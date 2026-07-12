// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/javascript/JavascriptParser.js

use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use crate::{
    AsyncDependenciesBlock, ConstDependency, DependenciesBlock, Dependency, Error,
    HarmonyExportExpressionDependency, HarmonyExportHeaderDependency,
    HarmonyExportImportedSpecifierDependency, HarmonyExportSpecifierDependency,
    HarmonyImportSideEffectDependency, HarmonyImportSpecifierDependency, ImportDependency, Result,
    SourceRange,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use swc_experimental_allocator::Allocator;
use swc_experimental_allocator::atom::Wtf8Atom;
use swc_experimental_ecma_ast::{
    ArrowExpr, AssignExpr, AwaitExpr, BindingIdent, BlockStmt, CallExpr, Callee, ClassDecl,
    ClassExpr, Decl, DefaultDecl, EsVersion, ExportSpecifier, Expr, FnDecl, FnExpr, Function,
    Ident, Lit, Module, ModuleDecl, ModuleExportName, ModuleItem, NewExpr, Pat, Prop, Stmt, Str,
    TaggedTpl, Tpl, UpdateExpr, VarDeclarator, Visit, VisitWith, YieldExpr,
};
use swc_experimental_ecma_parser::{EsSyntax, Syntax, parse_file_as_module};

const UNSUPPORTED_DYNAMIC_IMPORT_MESSAGE: &str =
    "only static string specifiers are supported; context modules are not supported yet";

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ParsedModule {
    pub dependencies_block: DependenciesBlock,
    pub presentational_dependencies: Vec<Dependency>,
    pub data: ParsedModuleData,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ParsedModuleData {
    #[default]
    JavaScript,
    Json(serde_json::Value),
    Asset {
        module_type: crate::ModuleType,
    },
}

impl Hash for ParsedModuleData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::JavaScript => 0_u8.hash(state),
            Self::Json(value) => {
                1_u8.hash(state);
                serde_json::to_string(value)
                    .expect("parsed JSON values must serialize")
                    .hash(state);
            }
            Self::Asset { module_type } => {
                2_u8.hash(state);
                module_type.hash(state);
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ImportBinding {
    request: String,
    source_order: usize,
    ids: Vec<String>,
    local: String,
}

pub(crate) fn parse_module_dependencies(path: &Path, source: &str) -> Result<ParsedModule> {
    parse_module_dependencies_sync(path, source)
}

pub(crate) fn source_is_side_effect_free(path: &Path, source: &str) -> bool {
    let allocator = Allocator::new();
    let Ok(module) = parse_file_as_module(
        &allocator,
        source,
        syntax_for_path(path),
        EsVersion::EsNext,
        None,
    ) else {
        return false;
    };

    let pure_functions = no_side_effects_functions(source);
    module
        .body
        .iter()
        .all(|item| module_item_is_side_effect_free(item, &pure_functions))
}

fn module_item_is_side_effect_free(
    item: &ModuleItem<'_>,
    pure_functions: &HashSet<String>,
) -> bool {
    match item {
        ModuleItem::ModuleDecl(declaration) => match &**declaration {
            ModuleDecl::Import(_) | ModuleDecl::ExportAll(_) | ModuleDecl::ExportNamed(_) => true,
            ModuleDecl::ExportDecl(declaration) => {
                declaration_is_side_effect_free(&declaration.decl, pure_functions)
            }
            ModuleDecl::ExportDefaultDecl(declaration) => match &declaration.decl {
                DefaultDecl::Fn(_) => true,
                DefaultDecl::Class(class) => {
                    let mut visitor = SideEffectsVisitor::new(pure_functions);
                    class.visit_with(&mut visitor);
                    !visitor.has_side_effects
                }
            },
            ModuleDecl::ExportDefaultExpr(expression) => {
                expression_is_side_effect_free(&expression.expr, pure_functions)
            }
        },
        ModuleItem::Stmt(statement) => match &**statement {
            Stmt::Empty(_) => true,
            Stmt::Decl(declaration) => declaration_is_side_effect_free(declaration, pure_functions),
            Stmt::Expr(expression) => {
                expression_is_side_effect_free(&expression.expr, pure_functions)
            }
            _ => false,
        },
    }
}

fn declaration_is_side_effect_free(
    declaration: &Decl<'_>,
    pure_functions: &HashSet<String>,
) -> bool {
    if matches!(declaration, Decl::Fn(_)) {
        return true;
    }
    let mut visitor = SideEffectsVisitor::new(pure_functions);
    declaration.visit_with(&mut visitor);
    !visitor.has_side_effects
}

fn expression_is_side_effect_free(expression: &Expr<'_>, pure_functions: &HashSet<String>) -> bool {
    let mut visitor = SideEffectsVisitor::new(pure_functions);
    expression.visit_with(&mut visitor);
    !visitor.has_side_effects
}

struct SideEffectsVisitor<'names> {
    has_side_effects: bool,
    pure_functions: &'names HashSet<String>,
}

impl<'names> SideEffectsVisitor<'names> {
    fn new(pure_functions: &'names HashSet<String>) -> Self {
        Self {
            has_side_effects: false,
            pure_functions,
        }
    }
}

impl<'a> Visit<'a> for SideEffectsVisitor<'_> {
    fn visit_arrow_expr(&mut self, _: &ArrowExpr<'a>) {}

    fn visit_function(&mut self, _: &Function<'a>) {}

    fn visit_fn_expr(&mut self, _: &FnExpr<'a>) {}

    fn visit_call_expr(&mut self, expression: &CallExpr<'a>) {
        let is_pure = match &expression.callee {
            Callee::Expr(callee) => match &**callee {
                Expr::Ident(identifier) => {
                    self.pure_functions.contains(&ident_to_string(identifier))
                }
                _ => false,
            },
            _ => false,
        };
        if !is_pure {
            self.has_side_effects = true;
            return;
        }
        for argument in expression.args.iter() {
            argument.expr.visit_with(self);
        }
    }

    fn visit_new_expr(&mut self, _: &NewExpr<'a>) {
        self.has_side_effects = true;
    }

    fn visit_assign_expr(&mut self, _: &AssignExpr<'a>) {
        self.has_side_effects = true;
    }

    fn visit_update_expr(&mut self, _: &UpdateExpr<'a>) {
        self.has_side_effects = true;
    }

    fn visit_await_expr(&mut self, _: &AwaitExpr<'a>) {
        self.has_side_effects = true;
    }

    fn visit_yield_expr(&mut self, _: &YieldExpr<'a>) {
        self.has_side_effects = true;
    }

    fn visit_tagged_tpl(&mut self, _: &TaggedTpl<'a>) {
        self.has_side_effects = true;
    }
}

fn no_side_effects_functions(source: &str) -> HashSet<String> {
    let annotation = r"/\*[#@]__NO_SIDE_EFFECTS__\*/";
    let identifier = r"([A-Za-z_$][A-Za-z0-9_$]*)";
    let before_declaration = Regex::new(&format!(
        r"(?s){annotation}\s*(?:function\s+{identifier}|(?:const|let|var)\s+{identifier})"
    ))
    .expect("NO_SIDE_EFFECTS declaration regex must compile");
    let inside_initializer = Regex::new(&format!(
        r"(?s)(?:const|let|var)\s+{identifier}\s*=\s*{annotation}"
    ))
    .expect("NO_SIDE_EFFECTS initializer regex must compile");
    let mut names = HashSet::new();
    for captures in before_declaration.captures_iter(source) {
        if let Some(name) = captures.get(1).or_else(|| captures.get(2)) {
            names.insert(name.as_str().to_string());
        }
    }
    for captures in inside_initializer.captures_iter(source) {
        if let Some(name) = captures.get(1) {
            names.insert(name.as_str().to_string());
        }
    }
    names
}

fn parse_module_dependencies_sync(path: &Path, source: &str) -> Result<ParsedModule> {
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

    let mut parsed = ParsedModule::default();
    let mut import_bindings = HashMap::new();
    collect_module_decl_dependencies(path, &module, &mut parsed, &mut import_bindings)?;
    collect_import_usages(
        &module,
        &import_bindings,
        &mut parsed.dependencies_block.dependencies,
    );
    collect_dynamic_import_dependencies(path, &module, &mut parsed)?;

    Ok(parsed)
}

fn collect_module_decl_dependencies(
    path: &Path,
    module: &Module<'_>,
    parsed: &mut ParsedModule,
    import_bindings: &mut HashMap<String, ImportBinding>,
) -> Result<()> {
    let mut source_order = 0;

    for item in module.body.iter() {
        let ModuleItem::ModuleDecl(decl) = item else {
            continue;
        };

        match &**decl {
            ModuleDecl::Import(import_decl) => {
                if import_decl.type_only {
                    continue;
                }

                source_order += 1;
                let request = specifier_to_string(path, &import_decl.src)?;
                parsed
                    .presentational_dependencies
                    .push(Dependency::Const(ConstDependency::new(
                        "",
                        range(import_decl.span),
                    )));
                parsed
                    .dependencies_block
                    .dependencies
                    .push(Dependency::HarmonyImportSideEffect(
                        HarmonyImportSideEffectDependency::new(
                            request.clone(),
                            source_order,
                            Some(range(import_decl.span)),
                        ),
                    ));

                for specifier in import_decl.specifiers.iter() {
                    match specifier {
                        swc_experimental_ecma_ast::ImportSpecifier::Named(named) => {
                            if named.is_type_only {
                                continue;
                            }
                            let local = ident_to_string(&named.local);
                            let imported = named
                                .imported
                                .as_ref()
                                .map(module_export_name_to_string)
                                .transpose()?
                                .unwrap_or_else(|| local.clone());
                            import_bindings.insert(
                                local.clone(),
                                ImportBinding {
                                    request: request.clone(),
                                    source_order,
                                    ids: vec![imported],
                                    local,
                                },
                            );
                        }
                        swc_experimental_ecma_ast::ImportSpecifier::Default(default) => {
                            let local = ident_to_string(&default.local);
                            import_bindings.insert(
                                local.clone(),
                                ImportBinding {
                                    request: request.clone(),
                                    source_order,
                                    ids: vec!["default".to_string()],
                                    local,
                                },
                            );
                        }
                        swc_experimental_ecma_ast::ImportSpecifier::Namespace(namespace) => {
                            let local = ident_to_string(&namespace.local);
                            import_bindings.insert(
                                local.clone(),
                                ImportBinding {
                                    request: request.clone(),
                                    source_order,
                                    ids: Vec::new(),
                                    local,
                                },
                            );
                        }
                    }
                }
            }
            ModuleDecl::ExportDecl(export_decl) => {
                parsed
                    .presentational_dependencies
                    .push(Dependency::HarmonyExportHeader(
                        HarmonyExportHeaderDependency::new(
                            Some(range(decl_span_for_export_decl(export_decl))),
                            range(export_decl.span),
                        ),
                    ));
                collect_decl_exports(
                    &export_decl.decl,
                    &mut parsed.dependencies_block.dependencies,
                );
            }
            ModuleDecl::ExportNamed(named_export) => {
                if named_export.type_only {
                    continue;
                }

                if let Some(src) = &named_export.src {
                    source_order += 1;
                    let request = specifier_to_string(path, src)?;
                    parsed.presentational_dependencies.push(Dependency::Const(
                        ConstDependency::new("", range(named_export.span)),
                    ));
                    parsed.dependencies_block.dependencies.push(
                        Dependency::HarmonyImportSideEffect(
                            HarmonyImportSideEffectDependency::new(
                                request.clone(),
                                source_order,
                                Some(range(named_export.span)),
                            ),
                        ),
                    );
                    for specifier in named_export.specifiers.iter() {
                        if let ExportSpecifier::Named(named) = specifier {
                            if named.is_type_only {
                                continue;
                            }
                            let orig = module_export_name_to_string(&named.orig)?;
                            let exported = named
                                .exported
                                .as_ref()
                                .map(module_export_name_to_string)
                                .transpose()?
                                .unwrap_or_else(|| orig.clone());
                            parsed.dependencies_block.dependencies.push(
                                Dependency::HarmonyExportImportedSpecifier(
                                    HarmonyExportImportedSpecifierDependency::new(
                                        request.clone(),
                                        source_order,
                                        vec![orig],
                                        Some(exported),
                                        false,
                                        Some(range(named.span)),
                                    ),
                                ),
                            );
                        }
                    }
                } else {
                    parsed
                        .presentational_dependencies
                        .push(Dependency::HarmonyExportHeader(
                            HarmonyExportHeaderDependency::new(None, range(named_export.span)),
                        ));
                    for specifier in named_export.specifiers.iter() {
                        if let ExportSpecifier::Named(named) = specifier {
                            if named.is_type_only {
                                continue;
                            }
                            let orig = module_export_name_to_string(&named.orig)?;
                            let exported = named
                                .exported
                                .as_ref()
                                .map(module_export_name_to_string)
                                .transpose()?
                                .unwrap_or_else(|| orig.clone());
                            if let Some(binding) = import_bindings.get(&orig) {
                                parsed.dependencies_block.dependencies.push(
                                    Dependency::HarmonyExportImportedSpecifier(
                                        HarmonyExportImportedSpecifierDependency::new(
                                            binding.request.clone(),
                                            binding.source_order,
                                            binding.ids.clone(),
                                            Some(exported),
                                            false,
                                            Some(range(named.span)),
                                        ),
                                    ),
                                );
                            } else {
                                parsed.dependencies_block.dependencies.push(
                                    Dependency::HarmonyExportSpecifier(
                                        HarmonyExportSpecifierDependency::new(orig, exported),
                                    ),
                                );
                            }
                        }
                    }
                }
            }
            ModuleDecl::ExportDefaultDecl(default_decl) => {
                let declaration_id = match &default_decl.decl {
                    DefaultDecl::Fn(function) => {
                        function.ident.as_ref().map(|id| ident_to_string(id))
                    }
                    DefaultDecl::Class(class) => class.ident.as_ref().map(|id| ident_to_string(id)),
                };
                parsed
                    .dependencies_block
                    .dependencies
                    .push(Dependency::HarmonyExportExpression(
                        HarmonyExportExpressionDependency::new(
                            range(default_decl_span(&default_decl.decl)),
                            range(default_decl.span),
                            declaration_id,
                        ),
                    ));
            }
            ModuleDecl::ExportDefaultExpr(default_expr) => {
                parsed
                    .dependencies_block
                    .dependencies
                    .push(Dependency::HarmonyExportExpression(
                        HarmonyExportExpressionDependency::new(
                            range(expr_span(&default_expr.expr)),
                            range(default_expr.span),
                            None,
                        ),
                    ));
            }
            ModuleDecl::ExportAll(export_all) => {
                if export_all.type_only {
                    continue;
                }
                source_order += 1;
                let request = specifier_to_string(path, &export_all.src)?;
                parsed
                    .presentational_dependencies
                    .push(Dependency::Const(ConstDependency::new(
                        "",
                        range(export_all.span),
                    )));
                parsed
                    .dependencies_block
                    .dependencies
                    .push(Dependency::HarmonyImportSideEffect(
                        HarmonyImportSideEffectDependency::new(
                            request.clone(),
                            source_order,
                            Some(range(export_all.span)),
                        ),
                    ));
                parsed.dependencies_block.dependencies.push(
                    Dependency::HarmonyExportImportedSpecifier(
                        HarmonyExportImportedSpecifierDependency::new(
                            request,
                            source_order,
                            Vec::new(),
                            None,
                            true,
                            Some(range(export_all.span)),
                        ),
                    ),
                );
            }
        }
    }

    Ok(())
}

fn collect_dynamic_import_dependencies(
    path: &Path,
    module: &Module<'_>,
    parsed: &mut ParsedModule,
) -> Result<()> {
    let mut visitor = DynamicImportVisitor {
        path,
        blocks: Vec::new(),
        error: None,
    };
    module.visit_with(&mut visitor);

    if let Some(error) = visitor.error {
        return Err(error);
    }

    parsed.dependencies_block.blocks.extend(visitor.blocks);
    Ok(())
}

struct DynamicImportVisitor<'path> {
    path: &'path Path,
    blocks: Vec<AsyncDependenciesBlock>,
    error: Option<Error>,
}

impl<'a> Visit<'a> for DynamicImportVisitor<'_> {
    fn visit_call_expr(&mut self, call: &CallExpr<'a>) {
        if matches!(call.callee, Callee::Import(_)) {
            match static_import_call_specifier(self.path, call) {
                Ok(Some((request, range))) => {
                    self.blocks
                        .push(AsyncDependenciesBlock::new(vec![Dependency::Import(
                            ImportDependency::new(request, range, None),
                        )]));
                }
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

fn collect_import_usages(
    module: &Module<'_>,
    import_bindings: &HashMap<String, ImportBinding>,
    dependencies: &mut Vec<Dependency>,
) {
    if import_bindings.is_empty() {
        return;
    }

    let mut visitor = ImportUsageVisitor {
        imports: import_bindings,
        dependencies: Vec::new(),
        scopes: vec![HashSet::new()],
    };
    module.visit_with(&mut visitor);
    dependencies.extend(visitor.dependencies);
}

struct ImportUsageVisitor<'imports> {
    imports: &'imports HashMap<String, ImportBinding>,
    dependencies: Vec<Dependency>,
    scopes: Vec<HashSet<String>>,
}

impl ImportUsageVisitor<'_> {
    fn is_shadowed(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }

    fn add_binding(&mut self, name: impl Into<String>) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.into());
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}

impl<'a> Visit<'a> for ImportUsageVisitor<'_> {
    fn visit_module_decl(&mut self, node: &ModuleDecl<'a>) {
        match node {
            ModuleDecl::ExportDecl(export_decl) => export_decl.decl.visit_with(self),
            ModuleDecl::ExportDefaultExpr(default_expr) => default_expr.expr.visit_with(self),
            _ => {}
        }
    }

    fn visit_block_stmt(&mut self, node: &BlockStmt<'a>) {
        self.push_scope();
        node.visit_children_with(self);
        self.pop_scope();
    }

    fn visit_fn_decl(&mut self, node: &FnDecl<'a>) {
        self.add_binding(ident_to_string(&node.ident));
        self.visit_function(&node.function);
    }

    fn visit_fn_expr(&mut self, node: &FnExpr<'a>) {
        self.push_scope();
        if let Some(ident) = &node.ident {
            self.add_binding(ident_to_string(ident));
        }
        self.add_function_params(&node.function);
        if let Some(body) = &node.function.body {
            body.visit_with(self);
        }
        self.pop_scope();
    }

    fn visit_arrow_expr(&mut self, node: &ArrowExpr<'a>) {
        self.push_scope();
        for param in node.params.iter() {
            add_pat_bindings(param, self.scopes.last_mut().expect("scope exists"));
        }
        node.visit_children_with(self);
        self.pop_scope();
    }

    fn visit_class_decl(&mut self, node: &ClassDecl<'a>) {
        self.add_binding(ident_to_string(&node.ident));
        node.class.visit_with(self);
    }

    fn visit_class_expr(&mut self, node: &ClassExpr<'a>) {
        self.push_scope();
        if let Some(ident) = &node.ident {
            self.add_binding(ident_to_string(ident));
        }
        node.class.visit_with(self);
        self.pop_scope();
    }

    fn visit_var_declarator(&mut self, node: &VarDeclarator<'a>) {
        add_pat_bindings(
            &node.name,
            self.scopes.last_mut().expect("scope stack should exist"),
        );
        if let Some(init) = &node.init {
            init.visit_with(self);
        }
    }

    fn visit_binding_ident(&mut self, _node: &BindingIdent<'a>) {}

    fn visit_prop(&mut self, node: &Prop<'a>) {
        if let Prop::Shorthand(ident) = node {
            self.add_import_usage(ident, true);
        } else {
            node.visit_children_with(self);
        }
    }

    fn visit_ident(&mut self, node: &Ident<'a>) {
        self.add_import_usage(node, false);
    }
}

impl ImportUsageVisitor<'_> {
    fn add_import_usage(&mut self, node: &Ident<'_>, shorthand: bool) {
        let name = ident_to_string(node);
        if let Some(binding) = self.imports.get(&name)
            && !self.is_shadowed(&name)
        {
            let mut dependency = HarmonyImportSpecifierDependency::new(
                binding.request.clone(),
                binding.source_order,
                binding.ids.clone(),
                binding.local.clone(),
                range(node.span),
            );
            dependency.shorthand = shorthand;
            self.dependencies
                .push(Dependency::HarmonyImportSpecifier(dependency));
        }
    }
}

impl ImportUsageVisitor<'_> {
    fn visit_function(&mut self, function: &Function<'_>) {
        self.push_scope();
        self.add_function_params(function);
        if let Some(body) = &function.body {
            body.visit_with(self);
        }
        self.pop_scope();
    }

    fn add_function_params(&mut self, function: &Function<'_>) {
        for param in function.params.iter() {
            add_pat_bindings(&param.pat, self.scopes.last_mut().expect("scope exists"));
        }
    }
}

fn add_pat_bindings(pattern: &Pat<'_>, bindings: &mut impl BindingCollector) {
    match pattern {
        Pat::Ident(ident) => {
            bindings.collect_binding(ident_to_string(&ident.id));
        }
        Pat::Array(array) => {
            for elem in array.elems.iter().flatten() {
                add_pat_bindings(elem, bindings);
            }
        }
        Pat::Rest(rest) => add_pat_bindings(&rest.arg, bindings),
        Pat::Object(object) => {
            for prop in object.props.iter() {
                match prop {
                    swc_experimental_ecma_ast::ObjectPatProp::KeyValue(key_value) => {
                        add_pat_bindings(&key_value.value, bindings);
                    }
                    swc_experimental_ecma_ast::ObjectPatProp::Assign(assign) => {
                        bindings.collect_binding(ident_to_string(&assign.key.id));
                    }
                    swc_experimental_ecma_ast::ObjectPatProp::Rest(rest) => {
                        add_pat_bindings(&rest.arg, bindings);
                    }
                }
            }
        }
        Pat::Assign(assign) => add_pat_bindings(&assign.left, bindings),
        Pat::Invalid(_) | Pat::Expr(_) => {}
    }
}

trait BindingCollector {
    fn collect_binding(&mut self, name: String);
}

impl BindingCollector for HashSet<String> {
    fn collect_binding(&mut self, name: String) {
        self.insert(name);
    }
}

impl BindingCollector for Vec<String> {
    fn collect_binding(&mut self, name: String) {
        if !self.contains(&name) {
            self.push(name);
        }
    }
}

fn collect_decl_exports(decl: &Decl<'_>, dependencies: &mut Vec<Dependency>) {
    match decl {
        Decl::Class(class) => {
            let name = ident_to_string(&class.ident);
            dependencies.push(Dependency::HarmonyExportSpecifier(
                HarmonyExportSpecifierDependency::new(name.clone(), name),
            ));
        }
        Decl::Fn(function) => {
            let name = ident_to_string(&function.ident);
            dependencies.push(Dependency::HarmonyExportSpecifier(
                HarmonyExportSpecifierDependency::new(name.clone(), name),
            ));
        }
        Decl::Var(var) => {
            let mut names = Vec::new();
            for declarator in var.decls.iter() {
                add_pat_bindings(&declarator.name, &mut names);
            }
            for name in names {
                dependencies.push(Dependency::HarmonyExportSpecifier(
                    HarmonyExportSpecifierDependency::new(name.clone(), name),
                ));
            }
        }
        Decl::Using(_) => {}
    }
}

fn static_import_call_specifier(
    path: &Path,
    call: &CallExpr<'_>,
) -> Result<Option<(String, SourceRange)>> {
    let Some(first_arg) = call.args.first() else {
        return Ok(None);
    };

    if first_arg.spread.is_some() {
        return Err(unsupported_dynamic_import(path));
    }

    match &first_arg.expr {
        Expr::Lit(lit) => match &**lit {
            Lit::Str(specifier) => specifier_to_string(path, specifier)
                .map(|request| Some((request, range(call.span)))),
            _ => Err(unsupported_dynamic_import(path)),
        },
        Expr::Tpl(template) => static_template_to_string(path, template)
            .map(|request| Some((request, range(call.span)))),
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

fn module_export_name_to_string(name: &ModuleExportName<'_>) -> Result<String> {
    match name {
        ModuleExportName::Ident(ident) => Ok(ident_to_string(ident)),
        ModuleExportName::Str(str) => wtf8_to_string(&str.value).ok_or_else(|| Error::Parse {
            path: PathBuf::new(),
            message: "module export name is not valid UTF-8".to_string(),
        }),
    }
}

fn ident_to_string(ident: &Ident<'_>) -> String {
    ident.sym.as_str().to_string()
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

fn range(span: swc_experimental_ecma_ast::Span) -> SourceRange {
    SourceRange::new(span.start.saturating_sub(1), span.end.saturating_sub(1))
}

fn decl_span_for_export_decl(
    export_decl: &swc_experimental_ecma_ast::ExportDecl<'_>,
) -> swc_experimental_ecma_ast::Span {
    match &export_decl.decl {
        Decl::Class(class) => class.class.span,
        Decl::Fn(function) => function.function.span,
        Decl::Var(var) => var.span,
        Decl::Using(using) => using.span,
    }
}

fn default_decl_span(default_decl: &DefaultDecl<'_>) -> swc_experimental_ecma_ast::Span {
    match default_decl {
        DefaultDecl::Class(class) => class.class.span,
        DefaultDecl::Fn(function) => function.function.span,
    }
}

fn expr_span(expr: &Expr<'_>) -> swc_experimental_ecma_ast::Span {
    match expr {
        Expr::This(expr) => expr.span,
        Expr::Array(expr) => expr.span,
        Expr::Object(expr) => expr.span,
        Expr::Fn(expr) => expr.function.span,
        Expr::Unary(expr) => expr.span,
        Expr::Update(expr) => expr.span,
        Expr::Bin(expr) => expr.span,
        Expr::Assign(expr) => expr.span,
        Expr::Member(expr) => expr.span,
        Expr::SuperProp(expr) => expr.span,
        Expr::Cond(expr) => expr.span,
        Expr::Call(expr) => expr.span,
        Expr::New(expr) => expr.span,
        Expr::Seq(expr) => expr.span,
        Expr::Ident(expr) => expr.span,
        Expr::Lit(expr) => lit_span(expr),
        Expr::Tpl(expr) => expr.span,
        Expr::TaggedTpl(expr) => expr.span,
        Expr::Arrow(expr) => expr.span,
        Expr::Class(expr) => expr.class.span,
        Expr::Yield(expr) => expr.span,
        Expr::MetaProp(expr) => expr.span,
        Expr::Await(expr) => expr.span,
        Expr::Paren(expr) => expr.span,
        Expr::JSXMember(expr) => expr.span,
        Expr::JSXNamespacedName(expr) => expr.span,
        Expr::JSXEmpty(expr) => expr.span,
        Expr::JSXElement(expr) => expr.span,
        Expr::JSXFragment(expr) => expr.span,
        Expr::PrivateName(expr) => expr.span,
        Expr::OptChain(expr) => expr.span,
        Expr::Invalid(_) => swc_experimental_ecma_ast::DUMMY_SP,
    }
}

fn lit_span(lit: &Lit<'_>) -> swc_experimental_ecma_ast::Span {
    match lit {
        Lit::Str(lit) => lit.span,
        Lit::Bool(lit) => lit.span,
        Lit::Null(lit) => lit.span,
        Lit::Num(lit) => lit.span,
        Lit::BigInt(lit) => lit.span,
        Lit::Regex(lit) => lit.span,
    }
}
