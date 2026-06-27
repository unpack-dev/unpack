use std::path::{Path, PathBuf};

use crate::{Dependency, DependencyKind, Error, Result};
use swc_experimental_allocator::Allocator;
use swc_experimental_ecma_ast::{EsVersion, ModuleDecl, ModuleItem, Str};
use swc_experimental_ecma_parser::{EsSyntax, Syntax, parse_file_as_module};

pub(crate) async fn parse_static_esm_dependencies(
    path: PathBuf,
    source: String,
) -> Result<Vec<Dependency>> {
    let task_path = path.clone();
    tokio::task::spawn_blocking(move || parse_static_esm_dependencies_sync(&path, &source))
        .await
        .map_err(|error| Error::ParseTask {
            path: task_path,
            message: error.to_string(),
        })?
}

fn parse_static_esm_dependencies_sync(path: &Path, source: &str) -> Result<Vec<Dependency>> {
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

fn specifier_to_string(path: &Path, specifier: &Str<'_>) -> Result<String> {
    specifier
        .value
        .as_wtf8()
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| Error::Parse {
            path: path.to_path_buf(),
            message: "module specifier is not valid UTF-8".to_string(),
        })
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
