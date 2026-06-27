use std::path::{Path, PathBuf};

use crate::{Dependency, DependencyKind, Error, Result};
use swc_experimental_allocator::Allocator;
use swc_experimental_allocator::atom::Wtf8Atom;
use swc_experimental_ecma_ast::{
    CallExpr, Callee, EsVersion, Expr, Lit, Module, ModuleDecl, ModuleItem, Str, Tpl, Visit,
    VisitWith,
};
use swc_experimental_ecma_parser::{EsSyntax, Syntax, parse_file_as_module};

const UNSUPPORTED_DYNAMIC_IMPORT_MESSAGE: &str =
    "only static string specifiers are supported; context modules are not supported yet";

pub(crate) async fn parse_module_dependencies(
    path: PathBuf,
    source: String,
) -> Result<Vec<Dependency>> {
    let task_path = path.clone();
    tokio::task::spawn_blocking(move || parse_module_dependencies_sync(&path, &source))
        .await
        .map_err(|error| Error::ParseTask {
            path: task_path,
            message: error.to_string(),
        })?
}

fn parse_module_dependencies_sync(path: &Path, source: &str) -> Result<Vec<Dependency>> {
    let allocator = Allocator::new();
    let module = parse_file_as_module(
        &allocator,
        source,
        syntax_for_path(path),
        EsVersion::EsNext,
        None,
    )
    .map_err(|error| {
        let diagnostic = error.into_diagnostic();
        Error::Parse {
            path: path.to_path_buf(),
            message: diagnostic.to_string(),
        }
    })?;

    let mut dependencies = collect_static_esm_dependencies(path, &module)?;
    dependencies.extend(collect_dynamic_import_dependencies(path, &module)?);

    Ok(dependencies)
}

fn collect_static_esm_dependencies(path: &Path, module: &Module<'_>) -> Result<Vec<Dependency>> {
    let mut dependencies = Vec::new();
    for item in module.body.iter() {
        let ModuleItem::ModuleDecl(decl) = item else {
            continue;
        };

        match &**decl {
            ModuleDecl::Import(import_decl) => {
                if !import_decl.type_only {
                    dependencies.push(Dependency::new(
                        DependencyKind::StaticImport,
                        specifier_to_string(path, &import_decl.src)?,
                    ));
                }
            }
            ModuleDecl::ExportNamed(named_export) => {
                if !named_export.type_only
                    && let Some(src) = &named_export.src
                {
                    dependencies.push(Dependency::new(
                        DependencyKind::StaticExport,
                        specifier_to_string(path, src)?,
                    ));
                }
            }
            ModuleDecl::ExportAll(export_all) => {
                if !export_all.type_only {
                    dependencies.push(Dependency::new(
                        DependencyKind::StaticExport,
                        specifier_to_string(path, &export_all.src)?,
                    ));
                }
            }
            ModuleDecl::ExportDecl(_)
            | ModuleDecl::ExportDefaultDecl(_)
            | ModuleDecl::ExportDefaultExpr(_) => {}
        }
    }

    Ok(dependencies)
}

fn collect_dynamic_import_dependencies(
    path: &Path,
    module: &Module<'_>,
) -> Result<Vec<Dependency>> {
    let mut visitor = DynamicImportVisitor {
        path,
        dependencies: Vec::new(),
        error: None,
    };
    module.visit_with(&mut visitor);

    if let Some(error) = visitor.error {
        return Err(error);
    }

    Ok(visitor.dependencies)
}

struct DynamicImportVisitor<'path> {
    path: &'path Path,
    dependencies: Vec<Dependency>,
    error: Option<Error>,
}

impl<'a> Visit<'a> for DynamicImportVisitor<'_> {
    fn visit_call_expr(&mut self, call: &CallExpr<'a>) {
        if matches!(call.callee, Callee::Import(_)) {
            match static_import_call_specifier(self.path, call) {
                Ok(Some(request)) => self
                    .dependencies
                    .push(Dependency::new(DependencyKind::DynamicImport, request)),
                Ok(None) => {}
                Err(error) => {
                    if self.error.is_none() {
                        self.error = Some(error);
                    }
                }
            }
        }

        call.visit_children_with(self);
    }
}

fn static_import_call_specifier(path: &Path, call: &CallExpr<'_>) -> Result<Option<String>> {
    let Some(first_arg) = call.args.first() else {
        return Ok(None);
    };

    if first_arg.spread.is_some() {
        return Err(unsupported_dynamic_import(path));
    }

    match &first_arg.expr {
        Expr::Lit(lit) => match &**lit {
            Lit::Str(specifier) => specifier_to_string(path, specifier).map(Some),
            _ => Err(unsupported_dynamic_import(path)),
        },
        Expr::Tpl(template) => static_template_to_string(path, template).map(Some),
        _ => Err(unsupported_dynamic_import(path)),
    }
}

fn static_template_to_string(path: &Path, template: &Tpl<'_>) -> Result<String> {
    if !template.exprs.is_empty() || template.quasis.len() != 1 {
        return Err(unsupported_dynamic_import(path));
    }

    template
        .quasis
        .first()
        .and_then(|quasi| quasi.cooked.as_ref())
        .and_then(wtf8_to_string)
        .ok_or_else(|| Error::Parse {
            path: path.to_path_buf(),
            message: "dynamic import template specifier is not valid UTF-8".to_string(),
        })
}

fn specifier_to_string(path: &Path, specifier: &Str<'_>) -> Result<String> {
    wtf8_to_string(&specifier.value).ok_or_else(|| Error::Parse {
        path: path.to_path_buf(),
        message: "module specifier is not valid UTF-8".to_string(),
    })
}

fn wtf8_to_string(value: &Wtf8Atom<'_>) -> Option<String> {
    value.as_wtf8().as_str().map(ToOwned::to_owned)
}

fn unsupported_dynamic_import(path: &Path) -> Error {
    Error::UnsupportedDynamicImport {
        path: path.to_path_buf(),
        message: UNSUPPORTED_DYNAMIC_IMPORT_MESSAGE.to_string(),
    }
}

fn syntax_for_path(path: &Path) -> Syntax {
    let jsx = matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("jsx" | "tsx")
    );

    Syntax::Es(EsSyntax {
        jsx,
        import_attributes: true,
        ..Default::default()
    })
}
