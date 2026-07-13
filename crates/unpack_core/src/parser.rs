// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/javascript/JavascriptParser.js

use std::{
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    AsyncDependenciesBlock, ConstDependency, DependenciesBlock, Dependency, Error,
    HarmonyExportExpressionDependency, HarmonyExportHeaderDependency,
    HarmonyExportImportedSpecifierDependency, HarmonyExportSpecifierDependency,
    HarmonyImportSideEffectDependency, HarmonyImportSpecifierDependency, ImportDependency,
    ModuleType, Result, SourceRange,
};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use swc_experimental_allocator::Allocator;
use swc_experimental_allocator::atom::Wtf8Atom;
use swc_experimental_ecma_ast::{
    ArrowExpr, BinaryOp, BindingIdent, BlockStmt, CallExpr, Callee, Class, ClassDecl, ClassExpr,
    ClassMember, Comments, Decl, DefaultDecl, EsVersion, ExportSpecifier, Expr, FnDecl, FnExpr,
    Function, GetSpan, Ident, Key, Lit, Module, ModuleDecl, ModuleExportName, ModuleItem,
    OptChainBase, Pat, Prop, PropName, PropOrSpread, Stmt, Str, Tpl, UnaryOp, VarDeclKind,
    VarDeclOrExpr, VarDeclarator, Visit, VisitWith,
};
use swc_experimental_ecma_parser::{EsSyntax, Syntax};

const UNSUPPORTED_DYNAMIC_IMPORT_MESSAGE: &str =
    "only static string specifiers are supported; context modules are not supported yet";

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ParsedModule {
    pub dependencies_block: DependenciesBlock,
    pub presentational_dependencies: Vec<Dependency>,
    pub data: ParsedModuleData,
    pub build_meta: JavascriptBuildMeta,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct JavascriptBuildMeta {
    pub side_effect_free: Option<bool>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ParsedModuleData {
    #[default]
    JavaScript,
    Json(serde_json::Value),
    Asset {
        module_type: ModuleType,
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

type ProgramTap = Arc<
    dyn for<'ast, 'context> Fn(
            &JavascriptParserContext<'ast, 'context>,
            &Module<'ast>,
            &mut ParsedModule,
        ) + Send
        + Sync,
>;
type StatementTap = Arc<
    dyn for<'parser, 'ast, 'context> Fn(
            &JavascriptParserStatement<'parser, 'ast, 'context>,
            &mut ParsedModule,
        ) + Send
        + Sync,
>;
#[derive(Default, Clone)]
pub(crate) struct JavascriptParserHookSet {
    pub program: JavascriptParserModuleHook,
    pub statement: JavascriptParserStatementHook,
    pub finish: JavascriptParserModuleHook,
    requires_pure_analysis: bool,
}

#[derive(Default, Clone)]
pub(crate) struct JavascriptParserModuleHook {
    taps: Vec<(&'static str, Vec<u8>, ProgramTap)>,
}

#[derive(Default, Clone)]
pub(crate) struct JavascriptParserStatementHook {
    taps: Vec<(&'static str, Vec<u8>, StatementTap)>,
}

pub(crate) struct JavascriptParserContext<'ast, 'context> {
    pure_analysis: Option<&'context PureAnalysis<'context, 'ast>>,
}

pub(crate) struct JavascriptParserStatement<'parser, 'ast, 'context> {
    parser: &'parser JavascriptParserContext<'ast, 'context>,
    item: &'parser ModuleItem<'ast>,
    comments_start: usize,
}

impl JavascriptParserContext<'_, '_> {
    fn is_pure(&self, item: &ModuleItem<'_>, comments_start: usize) -> bool {
        self.pure_analysis
            .expect("parser plugin must request pure analysis before querying it")
            .module_item_is_pure(item, comments_start)
    }
}

impl JavascriptParserStatement<'_, '_, '_> {
    #[allow(dead_code)]
    pub(crate) fn item(&self) -> &ModuleItem<'_> {
        self.item
    }

    pub(crate) fn is_pure(&self) -> bool {
        self.parser.is_pure(self.item, self.comments_start)
    }
}

impl JavascriptParserHookSet {
    pub(crate) fn require_pure_analysis(&mut self) {
        self.requires_pure_analysis = true;
    }

    pub(crate) fn cache_fingerprint(&self) -> Vec<u8> {
        let mut fingerprint = b"unpack/javascript-parser-hooks/1".to_vec();
        fn append_phase<'a>(
            fingerprint: &mut Vec<u8>,
            phase: &[u8],
            taps: impl ExactSizeIterator<Item = (&'a str, &'a [u8])>,
        ) {
            fingerprint.extend_from_slice(&(phase.len() as u64).to_le_bytes());
            fingerprint.extend_from_slice(phase);
            fingerprint.extend_from_slice(&(taps.len() as u64).to_le_bytes());
            for (name, cache_key) in taps {
                fingerprint.extend_from_slice(&(name.len() as u64).to_le_bytes());
                fingerprint.extend_from_slice(name.as_bytes());
                fingerprint.extend_from_slice(&(cache_key.len() as u64).to_le_bytes());
                fingerprint.extend_from_slice(cache_key);
            }
        }
        append_phase(&mut fingerprint, b"program", self.program.cache_keys());
        append_phase(&mut fingerprint, b"statement", self.statement.cache_keys());
        append_phase(&mut fingerprint, b"finish", self.finish.cache_keys());
        fingerprint.push(u8::from(self.requires_pure_analysis));
        fingerprint
    }
}

impl JavascriptParserModuleHook {
    pub(crate) fn tap(
        &mut self,
        name: &'static str,
        cache_key: impl AsRef<[u8]>,
        tap: impl for<'ast, 'context> Fn(
            &JavascriptParserContext<'ast, 'context>,
            &Module<'ast>,
            &mut ParsedModule,
        ) + Send
        + Sync
        + 'static,
    ) {
        self.taps
            .push((name, cache_key.as_ref().to_vec(), Arc::new(tap)));
    }

    fn call(
        &self,
        context: &JavascriptParserContext<'_, '_>,
        module: &Module<'_>,
        result: &mut ParsedModule,
    ) {
        for (_, _, tap) in &self.taps {
            tap(context, module, result);
        }
    }

    fn cache_keys(&self) -> impl ExactSizeIterator<Item = (&str, &[u8])> {
        self.taps
            .iter()
            .map(|(name, cache_key, _)| (*name, cache_key.as_slice()))
    }
}

impl JavascriptParserStatementHook {
    pub(crate) fn tap(
        &mut self,
        name: &'static str,
        cache_key: impl AsRef<[u8]>,
        tap: impl for<'parser, 'ast, 'context> Fn(
            &JavascriptParserStatement<'parser, 'ast, 'context>,
            &mut ParsedModule,
        ) + Send
        + Sync
        + 'static,
    ) {
        self.taps
            .push((name, cache_key.as_ref().to_vec(), Arc::new(tap)));
    }

    fn call(&self, statement: &JavascriptParserStatement<'_, '_, '_>, result: &mut ParsedModule) {
        for (_, _, tap) in &self.taps {
            tap(statement, result);
        }
    }

    fn cache_keys(&self) -> impl ExactSizeIterator<Item = (&str, &[u8])> {
        self.taps
            .iter()
            .map(|(name, cache_key, _)| (*name, cache_key.as_slice()))
    }
}

impl std::fmt::Debug for JavascriptParserHookSet {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JavascriptParserHookSet")
            .field("program_taps", &self.program.taps.len())
            .field("statement_taps", &self.statement.taps.len())
            .field("finish_taps", &self.finish.taps.len())
            .field("requires_pure_analysis", &self.requires_pure_analysis)
            .finish()
    }
}

#[derive(Debug, Clone)]
struct ImportBinding {
    request: String,
    source_order: usize,
    ids: Vec<String>,
    local: String,
}

pub(crate) fn parse_module_dependencies_with_hooks(
    path: &Path,
    source: &str,
    hooks: &JavascriptParserHookSet,
) -> Result<ParsedModule> {
    parse_module_dependencies_sync(path, source, hooks)
}

#[cfg(test)]
pub(crate) fn source_is_side_effect_free(path: &Path, source: &str) -> bool {
    let allocator = Allocator::new();
    let mut comments = Comments::new_in(&allocator);
    let Ok(module) = swc_experimental_ecma_parser::with_file_parser(
        &allocator,
        source,
        syntax_for_path(path),
        EsVersion::EsNext,
        Some(&mut comments),
        |parser| parser.parse_module(),
    ) else {
        return false;
    };

    PureAnalysis::new(&comments, &module).module_is_pure(&module)
}

struct PureAnalysis<'comments, 'arena> {
    comments: &'comments Comments<'arena>,
    pure_function_calls: FxHashSet<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PrimitiveKind {
    NonBigInt,
    BigInt,
    Mixed,
}

impl<'comments, 'arena> PureAnalysis<'comments, 'arena> {
    fn new(comments: &'comments Comments<'arena>, module: &Module<'_>) -> Self {
        let pure_functions = no_side_effects_functions(comments, module);
        let mut collector = PureFunctionCallCollector::new(comments, &pure_functions);
        module.visit_with(&mut collector);
        Self {
            comments,
            pure_function_calls: collector.calls,
        }
    }

    #[cfg(test)]
    fn module_is_pure(&self, module: &Module<'_>) -> bool {
        let mut comments_start = 0;
        module.body.iter().all(|item| {
            let pure = self.module_item_is_pure(item, comments_start);
            comments_start = span_end(item.span());
            pure
        })
    }

    fn module_item_is_pure(&self, item: &ModuleItem<'_>, comments_start: usize) -> bool {
        match item {
            ModuleItem::ModuleDecl(declaration) => match &**declaration {
                ModuleDecl::Import(_) | ModuleDecl::ExportAll(_) | ModuleDecl::ExportNamed(_) => {
                    true
                }
                ModuleDecl::ExportDecl(declaration) => self.declaration_is_pure(&declaration.decl),
                ModuleDecl::ExportDefaultDecl(declaration) => match &declaration.decl {
                    DefaultDecl::Fn(_) => true,
                    DefaultDecl::Class(class) => self.class_is_pure(&class.class),
                },
                ModuleDecl::ExportDefaultExpr(expression) => {
                    self.expression_is_pure(&expression.expr, comments_start)
                }
            },
            ModuleItem::Stmt(statement) => self.statement_is_pure(statement, comments_start),
        }
    }

    fn statements_are_pure<'a>(
        &self,
        mut statements: impl Iterator<Item = &'a Stmt<'a>>,
        mut comments_start: usize,
    ) -> bool {
        statements.all(|statement| {
            let pure = self.statement_is_pure(statement, comments_start);
            comments_start = span_end(statement.span());
            pure
        })
    }

    fn statement_is_pure(&self, statement: &Stmt<'_>, comments_start: usize) -> bool {
        match statement {
            Stmt::Block(block) => {
                self.statements_are_pure(block.stmts.iter(), span_start(block.span))
            }
            Stmt::Empty(_) => true,
            Stmt::Labeled(statement) => {
                self.statement_is_pure(&statement.body, span_start(statement.span))
            }
            Stmt::If(statement) => {
                self.expression_is_pure(&statement.test, comments_start)
                    && self.statement_is_pure(&statement.cons, span_end(statement.test.span()))
                    && statement.alt.as_ref().is_none_or(|alternate| {
                        self.statement_is_pure(alternate, span_end(statement.cons.span()))
                    })
            }
            Stmt::Switch(statement) => {
                self.expression_is_pure(&statement.discriminant, comments_start)
                    && statement.cases.iter().all(|case| {
                        self.statements_are_pure(
                            case.cons.iter(),
                            case.test
                                .as_ref()
                                .map_or(span_start(case.span), |test| span_end(test.span())),
                        )
                    })
            }
            Stmt::While(statement) => {
                self.expression_is_pure(&statement.test, comments_start)
                    && self.statement_is_pure(&statement.body, span_end(statement.test.span()))
            }
            Stmt::DoWhile(statement) => {
                self.statement_is_pure(&statement.body, comments_start)
                    && self.expression_is_pure(&statement.test, span_end(statement.body.span()))
            }
            Stmt::For(statement) => {
                let mut next_comments_start = comments_start;
                let init_is_pure = statement.init.as_ref().is_none_or(|init| {
                    let pure = match init {
                        VarDeclOrExpr::VarDecl(declaration) => {
                            self.variable_declaration_is_pure(declaration)
                        }
                        VarDeclOrExpr::Expr(expression) => {
                            self.expression_is_pure(expression, next_comments_start)
                        }
                    };
                    next_comments_start = span_end(init.span());
                    pure
                });
                let test_is_pure = statement.test.as_ref().is_none_or(|test| {
                    let pure = self.expression_is_pure(test, next_comments_start);
                    next_comments_start = span_end(test.span());
                    pure
                });
                let update_is_pure = statement.update.as_ref().is_none_or(|update| {
                    let pure = self.expression_is_pure(update, next_comments_start);
                    next_comments_start = span_end(update.span());
                    pure
                });
                init_is_pure
                    && test_is_pure
                    && update_is_pure
                    && self.statement_is_pure(&statement.body, next_comments_start)
            }
            Stmt::Decl(declaration) => self.declaration_is_pure(declaration),
            Stmt::Expr(expression) => self.expression_is_pure(&expression.expr, comments_start),
            _ => false,
        }
    }

    fn declaration_is_pure(&self, declaration: &Decl<'_>) -> bool {
        match declaration {
            Decl::Fn(_) => true,
            Decl::Class(class) => self.class_is_pure(&class.class),
            Decl::Var(declaration) => self.variable_declaration_is_pure(declaration),
            Decl::Using(_) => false,
        }
    }

    fn variable_declaration_is_pure(
        &self,
        declaration: &swc_experimental_ecma_ast::VarDecl<'_>,
    ) -> bool {
        declaration.decls.iter().all(|declarator| {
            declarator.init.as_ref().is_none_or(|initializer| {
                self.expression_is_pure(initializer, span_start(declarator.span))
            })
        })
    }

    fn expression_is_pure(&self, expression: &Expr<'_>, comments_start: usize) -> bool {
        match expression {
            Expr::This(_)
            | Expr::Fn(_)
            | Expr::Arrow(_)
            | Expr::Ident(_)
            | Expr::Lit(_)
            | Expr::MetaProp(_)
            | Expr::PrivateName(_) => true,
            Expr::Array(array) => {
                let mut next_comments_start = comments_start;
                array.elems.iter().all(|element| {
                    let Some(element) = element else {
                        return true;
                    };
                    if element.spread.is_some() {
                        return false;
                    }
                    let pure = self.expression_is_pure(&element.expr, next_comments_start);
                    next_comments_start = span_end(element.expr.span());
                    pure
                })
            }
            Expr::Object(object) => {
                let mut next_comments_start = comments_start;
                object.props.iter().all(|property| {
                    let PropOrSpread::Prop(property) = property else {
                        return false;
                    };
                    let pure = self.property_is_pure(property, next_comments_start);
                    next_comments_start = span_end(property.span());
                    pure
                })
            }
            Expr::Unary(unary) => match unary.op {
                UnaryOp::TypeOf | UnaryOp::Void | UnaryOp::Bang => {
                    self.expression_is_pure(&unary.arg, comments_start)
                }
                UnaryOp::Minus | UnaryOp::Plus | UnaryOp::Tilde => {
                    self.expression_is_known_primitive(&unary.arg)
                        && self.expression_is_pure(&unary.arg, comments_start)
                }
                UnaryOp::Delete => false,
            },
            Expr::Bin(binary) => match binary.op {
                BinaryOp::EqEqEq | BinaryOp::NotEqEq => {
                    self.expression_is_pure(&binary.left, comments_start)
                        && self.expression_is_pure(&binary.right, span_end(binary.left.span()))
                }
                BinaryOp::LogicalOr | BinaryOp::LogicalAnd | BinaryOp::NullishCoalescing => {
                    self.expression_is_pure(&binary.left, comments_start)
                        && self.expression_is_pure(&binary.right, span_end(binary.left.span()))
                }
                BinaryOp::In | BinaryOp::InstanceOf => false,
                _ => {
                    self.binary_operands_are_safe(binary.op, &binary.left, &binary.right)
                        && self.expression_is_pure(&binary.left, comments_start)
                        && self.expression_is_pure(&binary.right, span_end(binary.left.span()))
                }
            },
            Expr::Cond(conditional) => {
                self.expression_is_pure(&conditional.test, comments_start)
                    && self.expression_is_pure(&conditional.cons, span_end(conditional.test.span()))
                    && self.expression_is_pure(&conditional.alt, span_end(conditional.cons.span()))
            }
            Expr::Seq(sequence) => {
                let mut next_comments_start = comments_start;
                sequence.exprs.iter().all(|expression| {
                    let pure = self.expression_is_pure(expression, next_comments_start);
                    next_comments_start = span_end(expression.span());
                    pure
                })
            }
            Expr::Call(call) => {
                let named_pure = self.pure_function_calls.contains(&call.span.start);
                if !named_pure && !self.has_pure_annotation(comments_start, span_start(call.span)) {
                    return false;
                }
                self.arguments_are_pure(
                    call.args.iter(),
                    match &call.callee {
                        Callee::Expr(callee) => span_end(callee.span()),
                        Callee::Super(super_) => span_end(super_.span),
                        Callee::Import(import) => span_end(import.span),
                    },
                )
            }
            Expr::New(new) => {
                self.has_pure_annotation(comments_start, span_start(new.span))
                    && new.args.as_ref().is_none_or(|arguments| {
                        self.arguments_are_pure(arguments.iter(), span_end(new.callee.span()))
                    })
            }
            Expr::Tpl(template) => {
                let mut next_comments_start = comments_start;
                template.exprs.iter().all(|expression| {
                    let pure = self.expression_is_pure(expression, next_comments_start);
                    next_comments_start = span_end(expression.span());
                    pure
                })
            }
            Expr::TaggedTpl(tagged) => {
                self.has_pure_annotation(comments_start, span_start(tagged.span)) && {
                    let mut next_comments_start = span_end(tagged.tag.span());
                    tagged.tpl.exprs.iter().all(|expression| {
                        let pure = self.expression_is_pure(expression, next_comments_start);
                        next_comments_start = span_end(expression.span());
                        pure
                    })
                }
            }
            Expr::Class(class) => self.class_is_pure(&class.class),
            Expr::Paren(parenthesized) => {
                self.expression_is_pure(&parenthesized.expr, comments_start)
            }
            Expr::OptChain(chain) => match &chain.base {
                OptChainBase::Member(_) => false,
                OptChainBase::Call(call) => {
                    self.has_pure_annotation(comments_start, span_start(chain.span))
                        && self.arguments_are_pure(call.args.iter(), span_end(call.callee.span()))
                }
            },
            Expr::Update(_)
            | Expr::Assign(_)
            | Expr::Member(_)
            | Expr::SuperProp(_)
            | Expr::Yield(_)
            | Expr::Await(_)
            | Expr::JSXMember(_)
            | Expr::JSXNamespacedName(_)
            | Expr::JSXEmpty(_)
            | Expr::JSXElement(_)
            | Expr::JSXFragment(_)
            | Expr::Invalid(_) => false,
        }
    }

    fn arguments_are_pure<'a>(
        &self,
        mut arguments: impl Iterator<Item = &'a swc_experimental_ecma_ast::ExprOrSpread<'a>>,
        mut comments_start: usize,
    ) -> bool {
        arguments.all(|argument| {
            if argument.spread.is_some() {
                return false;
            }
            let pure = self.expression_is_pure(&argument.expr, comments_start);
            comments_start = span_end(argument.expr.span());
            pure
        })
    }

    fn property_is_pure(&self, property: &Prop<'_>, comments_start: usize) -> bool {
        match property {
            Prop::Shorthand(_) => true,
            Prop::KeyValue(property) => {
                self.property_name_is_pure(&property.key, comments_start)
                    && self.expression_is_pure(&property.value, span_end(property.key.span()))
            }
            Prop::Assign(property) => {
                self.expression_is_pure(&property.value, span_end(property.key.span))
            }
            Prop::Getter(property) => self.property_name_is_pure(&property.key, comments_start),
            Prop::Setter(property) => self.property_name_is_pure(&property.key, comments_start),
            Prop::Method(property) => self.property_name_is_pure(&property.key, comments_start),
        }
    }

    fn property_name_is_pure(&self, name: &PropName<'_>, comments_start: usize) -> bool {
        match name {
            PropName::Computed(computed) => self.expression_is_pure(&computed.expr, comments_start),
            _ => true,
        }
    }

    fn class_is_pure(&self, class: &Class<'_>) -> bool {
        if !class.decorators.is_empty()
            || class.super_class.as_ref().is_some_and(|super_class| {
                !self.expression_is_pure(super_class, span_start(class.span))
            })
        {
            return false;
        }

        class.body.iter().all(|member| match member {
            ClassMember::Constructor(_) => class.super_class.is_none(),
            ClassMember::Method(method) => {
                self.property_name_is_pure(&method.key, span_start(method.span))
            }
            ClassMember::PrivateMethod(_) | ClassMember::Empty(_) => true,
            ClassMember::ClassProp(property) => {
                property.decorators.is_empty()
                    && self.property_name_is_pure(&property.key, span_start(property.span))
                    && (!property.is_static
                        || property.value.as_ref().is_none_or(|value| {
                            self.expression_is_pure(value, span_end(property.key.span()))
                        }))
            }
            ClassMember::PrivateProp(property) => {
                property.decorators.is_empty()
                    && (!property.is_static
                        || property.value.as_ref().is_none_or(|value| {
                            self.expression_is_pure(value, span_end(property.key.span))
                        }))
            }
            ClassMember::StaticBlock(_) => false,
            ClassMember::AutoAccessor(accessor) => {
                accessor.decorators.is_empty()
                    && match &accessor.key {
                        Key::Private(_) => true,
                        Key::Public(name) => {
                            self.property_name_is_pure(name, span_start(accessor.span))
                        }
                    }
                    && (!accessor.is_static
                        || accessor.value.as_ref().is_none_or(|value| {
                            self.expression_is_pure(value, span_start(accessor.span))
                        }))
            }
        })
    }

    fn expression_is_known_primitive(&self, expression: &Expr<'_>) -> bool {
        self.primitive_kind(expression).is_some()
    }

    fn primitive_kind(&self, expression: &Expr<'_>) -> Option<PrimitiveKind> {
        match expression {
            Expr::Lit(literal) => match &**literal {
                Lit::BigInt(_) => Some(PrimitiveKind::BigInt),
                Lit::Regex(_) => None,
                _ => Some(PrimitiveKind::NonBigInt),
            },
            Expr::Tpl(template) => template
                .exprs
                .iter()
                .all(|expression| self.primitive_kind(expression).is_some())
                .then_some(PrimitiveKind::NonBigInt),
            Expr::Unary(unary) => match unary.op {
                UnaryOp::TypeOf | UnaryOp::Void | UnaryOp::Bang => Some(PrimitiveKind::NonBigInt),
                UnaryOp::Minus | UnaryOp::Tilde => self.primitive_kind(&unary.arg),
                UnaryOp::Plus => match self.primitive_kind(&unary.arg)? {
                    PrimitiveKind::BigInt | PrimitiveKind::Mixed => None,
                    PrimitiveKind::NonBigInt => Some(PrimitiveKind::NonBigInt),
                },
                UnaryOp::Delete => None,
            },
            Expr::Bin(binary) => match binary.op {
                BinaryOp::EqEq
                | BinaryOp::NotEq
                | BinaryOp::EqEqEq
                | BinaryOp::NotEqEq
                | BinaryOp::Lt
                | BinaryOp::LtEq
                | BinaryOp::Gt
                | BinaryOp::GtEq => Some(PrimitiveKind::NonBigInt),
                BinaryOp::LogicalOr | BinaryOp::LogicalAnd | BinaryOp::NullishCoalescing => {
                    merge_primitive_kinds(
                        self.primitive_kind(&binary.left)?,
                        self.primitive_kind(&binary.right)?,
                    )
                }
                BinaryOp::In | BinaryOp::InstanceOf => None,
                _ => self.binary_result_kind(binary.op, &binary.left, &binary.right),
            },
            Expr::Cond(conditional) => merge_primitive_kinds(
                self.primitive_kind(&conditional.cons)?,
                self.primitive_kind(&conditional.alt)?,
            ),
            Expr::Seq(sequence) => sequence
                .exprs
                .last()
                .and_then(|expression| self.primitive_kind(expression)),
            Expr::Paren(parenthesized) => self.primitive_kind(&parenthesized.expr),
            _ => None,
        }
    }

    fn binary_operands_are_safe(
        &self,
        operator: BinaryOp,
        left: &Expr<'_>,
        right: &Expr<'_>,
    ) -> bool {
        self.binary_result_kind(operator, left, right).is_some()
    }

    fn binary_result_kind(
        &self,
        operator: BinaryOp,
        left_expression: &Expr<'_>,
        right_expression: &Expr<'_>,
    ) -> Option<PrimitiveKind> {
        let left = self.primitive_kind(left_expression)?;
        let right = self.primitive_kind(right_expression)?;

        if matches!(operator, BinaryOp::EqEq | BinaryOp::NotEq)
            || matches!(
                operator,
                BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq
            )
        {
            return Some(PrimitiveKind::NonBigInt);
        }

        if left == PrimitiveKind::Mixed
            || right == PrimitiveKind::Mixed
            || (left == PrimitiveKind::BigInt) != (right == PrimitiveKind::BigInt)
        {
            return None;
        }

        if left == PrimitiveKind::BigInt {
            match operator {
                BinaryOp::Div if !bigint_literal_is_nonzero(right_expression) => {
                    return None;
                }
                BinaryOp::Mod => return None,
                BinaryOp::Exp if bigint_literal(right_expression).is_none() => {
                    return None;
                }
                BinaryOp::ZeroFillRShift => return None,
                _ => {}
            }
        }

        Some(left)
    }

    fn has_pure_annotation(&self, comments_start: usize, expression_start: usize) -> bool {
        has_compiler_hint(self.comments, comments_start, expression_start, "PURE")
    }
}

fn merge_primitive_kinds(left: PrimitiveKind, right: PrimitiveKind) -> Option<PrimitiveKind> {
    Some(if left == right {
        left
    } else {
        PrimitiveKind::Mixed
    })
}

fn bigint_literal_is_nonzero(expression: &Expr<'_>) -> bool {
    bigint_literal(expression).is_some_and(|value| value.value.as_str() != "0")
}

fn bigint_literal<'expression>(
    expression: &'expression Expr<'_>,
) -> Option<&'expression swc_experimental_ecma_ast::BigInt<'expression>> {
    match expression {
        Expr::Lit(literal) => match &**literal {
            Lit::BigInt(value) => Some(value),
            _ => None,
        },
        Expr::Paren(parenthesized) => bigint_literal(&parenthesized.expr),
        _ => None,
    }
}

fn span_start(span: swc_experimental_ecma_ast::Span) -> usize {
    span.start.saturating_sub(1) as usize
}

fn span_end(span: swc_experimental_ecma_ast::Span) -> usize {
    span.end.saturating_sub(1) as usize
}

fn no_side_effects_functions(comments: &Comments<'_>, module: &Module<'_>) -> FxHashSet<String> {
    let mut candidates = FxHashSet::default();
    let mut comments_start = 0;
    for item in module.body.iter() {
        let statement_start = span_start(item.span());
        match item {
            ModuleItem::Stmt(statement) => {
                if let Stmt::Decl(declaration) = &**statement {
                    collect_no_side_effects_declaration(
                        comments,
                        declaration,
                        comments_start,
                        statement_start,
                        &mut candidates,
                    );
                }
            }
            ModuleItem::ModuleDecl(declaration) => {
                match &**declaration {
                    ModuleDecl::ExportDecl(declaration) => collect_no_side_effects_declaration(
                        comments,
                        &declaration.decl,
                        comments_start,
                        statement_start,
                        &mut candidates,
                    ),
                    ModuleDecl::ExportDefaultDecl(declaration) => {
                        if let DefaultDecl::Fn(function) = &declaration.decl
                            && has_compiler_hint(
                                comments,
                                comments_start,
                                statement_start,
                                "NO_SIDE_EFFECTS",
                            )
                        {
                            candidates.insert(function.ident.as_ref().map_or_else(
                                || "default".to_string(),
                                |ident| ident_to_string(ident),
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
        comments_start = span_end(item.span());
    }

    let mut nested_var_collector = AnnotatedTopLevelVarCollector {
        comments,
        names: FxHashSet::default(),
    };
    module.visit_with(&mut nested_var_collector);
    candidates.extend(nested_var_collector.names);

    candidates
}

fn collect_no_side_effects_declaration(
    comments: &Comments<'_>,
    declaration: &Decl<'_>,
    comments_start: usize,
    statement_start: usize,
    candidates: &mut FxHashSet<String>,
) {
    match declaration {
        Decl::Fn(function) => {
            if has_compiler_hint(comments, comments_start, statement_start, "NO_SIDE_EFFECTS") {
                candidates.insert(ident_to_string(&function.ident));
            }
        }
        Decl::Var(declaration) => {
            for declarator in declaration.decls.iter() {
                let (Pat::Ident(binding), Some(initializer)) = (&declarator.name, &declarator.init)
                else {
                    continue;
                };
                if !matches!(initializer, Expr::Fn(_) | Expr::Arrow(_)) {
                    continue;
                }
                let before_declaration = declaration.kind == VarDeclKind::Const
                    && has_compiler_hint(
                        comments,
                        comments_start,
                        statement_start,
                        "NO_SIDE_EFFECTS",
                    );
                let before_initializer = has_compiler_hint(
                    comments,
                    span_end(binding.id.span),
                    span_start(initializer.span()),
                    "NO_SIDE_EFFECTS",
                );
                if before_declaration || before_initializer {
                    candidates.insert(ident_to_string(&binding.id));
                }
            }
        }
        Decl::Class(_) | Decl::Using(_) => {}
    }
}

struct AnnotatedTopLevelVarCollector<'comments, 'arena> {
    comments: &'comments Comments<'arena>,
    names: FxHashSet<String>,
}

impl<'a> Visit<'a> for AnnotatedTopLevelVarCollector<'_, '_> {
    fn visit_var_decl(&mut self, node: &swc_experimental_ecma_ast::VarDecl<'a>) {
        if node.kind == VarDeclKind::Var {
            for declarator in node.decls.iter() {
                let (Pat::Ident(binding), Some(initializer)) = (&declarator.name, &declarator.init)
                else {
                    continue;
                };
                if matches!(initializer, Expr::Fn(_) | Expr::Arrow(_))
                    && has_compiler_hint(
                        self.comments,
                        span_end(binding.id.span),
                        span_start(initializer.span()),
                        "NO_SIDE_EFFECTS",
                    )
                {
                    self.names.insert(ident_to_string(&binding.id));
                }
            }
        }
        node.visit_children_with(self);
    }

    fn visit_fn_decl(&mut self, _: &FnDecl<'a>) {}

    fn visit_fn_expr(&mut self, _: &FnExpr<'a>) {}

    fn visit_arrow_expr(&mut self, _: &ArrowExpr<'a>) {}

    fn visit_function(&mut self, _: &Function<'a>) {}

    fn visit_class_decl(&mut self, _: &ClassDecl<'a>) {}

    fn visit_class_expr(&mut self, _: &ClassExpr<'a>) {}
}

fn has_compiler_hint(
    comments: &Comments<'_>,
    range_start: usize,
    range_end: usize,
    hint: &str,
) -> bool {
    if range_start >= range_end {
        return false;
    }
    comments
        .leading
        .values()
        .chain(comments.trailing.values())
        .flatten()
        .any(|comment| {
            comment.kind == swc_experimental_ecma_ast::CommentKind::Block
                && span_start(comment.span) >= range_start
                && span_end(comment.span) <= range_end
                && compiler_hint_matches(comment.text.as_str(), hint)
        })
}

fn compiler_hint_matches(comment: &str, hint: &str) -> bool {
    let comment = comment.trim();
    match hint {
        "PURE" => matches!(comment, "#__PURE__" | "@__PURE__"),
        "NO_SIDE_EFFECTS" => {
            matches!(comment, "#__NO_SIDE_EFFECTS__" | "@__NO_SIDE_EFFECTS__")
        }
        _ => false,
    }
}

struct PureFunctionCallCollector<'comments, 'arena, 'functions> {
    comments: &'comments Comments<'arena>,
    pure_functions: &'functions FxHashSet<String>,
    scopes: Vec<FxHashMap<String, bool>>,
    calls: FxHashSet<u32>,
}

impl<'comments, 'arena, 'functions> PureFunctionCallCollector<'comments, 'arena, 'functions> {
    fn new(
        comments: &'comments Comments<'arena>,
        pure_functions: &'functions FxHashSet<String>,
    ) -> Self {
        Self {
            comments,
            pure_functions,
            scopes: Vec::new(),
            calls: FxHashSet::default(),
        }
    }

    fn push_scope(&mut self, bindings: FxHashMap<String, bool>) {
        self.scopes.push(bindings);
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn is_pure_binding(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
            .unwrap_or_else(|| self.pure_functions.contains(name))
    }
}

impl<'a> Visit<'a> for PureFunctionCallCollector<'_, '_, '_> {
    fn visit_block_stmt(&mut self, node: &BlockStmt<'a>) {
        self.push_scope(direct_statement_bindings(
            self.comments,
            node.stmts.iter(),
            span_start(node.span),
        ));
        node.visit_children_with(self);
        self.pop_scope();
    }

    fn visit_fn_decl(&mut self, _: &FnDecl<'a>) {}

    fn visit_fn_expr(&mut self, _: &FnExpr<'a>) {}

    fn visit_arrow_expr(&mut self, _: &ArrowExpr<'a>) {}

    fn visit_class_expr(&mut self, node: &ClassExpr<'a>) {
        let mut bindings = FxHashMap::default();
        if let Some(identifier) = &node.ident {
            bindings.insert(ident_to_string(identifier), false);
        }
        self.push_scope(bindings);
        node.class.visit_with(self);
        self.pop_scope();
    }

    fn visit_for_stmt(&mut self, node: &swc_experimental_ecma_ast::ForStmt<'a>) {
        let bindings = match &node.init {
            Some(VarDeclOrExpr::VarDecl(declaration)) => variable_scope_bindings(
                self.comments,
                declaration,
                span_start(node.span),
                span_start(declaration.span),
            ),
            _ => FxHashMap::default(),
        };
        self.push_scope(bindings);
        node.visit_children_with(self);
        self.pop_scope();
    }

    fn visit_for_in_stmt(&mut self, node: &swc_experimental_ecma_ast::ForInStmt<'a>) {
        self.visit_for_head_scope(&node.left, |collector| node.visit_children_with(collector));
    }

    fn visit_for_of_stmt(&mut self, node: &swc_experimental_ecma_ast::ForOfStmt<'a>) {
        self.visit_for_head_scope(&node.left, |collector| node.visit_children_with(collector));
    }

    fn visit_switch_stmt(&mut self, node: &swc_experimental_ecma_ast::SwitchStmt<'a>) {
        let mut bindings = FxHashMap::default();
        for case in node.cases.iter() {
            bindings.extend(direct_statement_bindings(
                self.comments,
                case.cons.iter(),
                span_start(case.span),
            ));
        }
        self.push_scope(bindings);
        node.visit_children_with(self);
        self.pop_scope();
    }

    fn visit_call_expr(&mut self, node: &CallExpr<'a>) {
        if let Callee::Expr(callee) = &node.callee
            && let Expr::Ident(identifier) = &**callee
            && self.is_pure_binding(identifier.sym.as_str())
        {
            self.calls.insert(node.span.start);
        }
        node.visit_children_with(self);
    }
}

impl PureFunctionCallCollector<'_, '_, '_> {
    fn visit_for_head_scope<'a>(
        &mut self,
        head: &swc_experimental_ecma_ast::ForHead<'a>,
        visit: impl FnOnce(&mut Self),
    ) {
        let mut bindings = FxHashMap::default();
        match head {
            swc_experimental_ecma_ast::ForHead::VarDecl(declaration) => {
                bindings.extend(variable_scope_bindings(
                    self.comments,
                    declaration,
                    span_start(declaration.span),
                    span_start(declaration.span),
                ));
            }
            swc_experimental_ecma_ast::ForHead::Pat(pattern) => {
                let mut names = FxHashSet::default();
                add_pat_bindings(pattern, &mut names);
                bindings.extend(names.into_iter().map(|name| (name, false)));
            }
            swc_experimental_ecma_ast::ForHead::UsingDecl(declaration) => {
                for declarator in declaration.decls.iter() {
                    let mut names = FxHashSet::default();
                    add_pat_bindings(&declarator.name, &mut names);
                    bindings.extend(names.into_iter().map(|name| (name, false)));
                }
            }
        }
        self.push_scope(bindings);
        visit(self);
        self.pop_scope();
    }
}

fn direct_statement_bindings<'a>(
    comments: &Comments<'_>,
    statements: impl Iterator<Item = &'a Stmt<'a>>,
    mut comments_start: usize,
) -> FxHashMap<String, bool> {
    let mut bindings = FxHashMap::default();
    for statement in statements {
        if let Stmt::Decl(declaration) = statement {
            match &**declaration {
                Decl::Fn(function) => {
                    bindings.insert(
                        ident_to_string(&function.ident),
                        has_compiler_hint(
                            comments,
                            comments_start,
                            span_start(statement.span()),
                            "NO_SIDE_EFFECTS",
                        ),
                    );
                }
                Decl::Class(class) => {
                    bindings.insert(ident_to_string(&class.ident), false);
                }
                Decl::Var(declaration) => {
                    bindings.extend(variable_scope_bindings(
                        comments,
                        declaration,
                        comments_start,
                        span_start(statement.span()),
                    ));
                }
                Decl::Using(declaration) => {
                    for declarator in declaration.decls.iter() {
                        let mut names = FxHashSet::default();
                        add_pat_bindings(&declarator.name, &mut names);
                        bindings.extend(names.into_iter().map(|name| (name, false)));
                    }
                }
            }
        }
        comments_start = span_end(statement.span());
    }
    bindings
}

fn variable_scope_bindings(
    comments: &Comments<'_>,
    declaration: &swc_experimental_ecma_ast::VarDecl<'_>,
    comments_start: usize,
    statement_start: usize,
) -> FxHashMap<String, bool> {
    let mut bindings = FxHashMap::default();
    if declaration.kind == VarDeclKind::Var {
        return bindings;
    }
    for declarator in declaration.decls.iter() {
        let mut names = FxHashSet::default();
        add_pat_bindings(&declarator.name, &mut names);
        bindings.extend(names.into_iter().map(|name| (name, false)));

        let (Pat::Ident(binding), Some(initializer)) = (&declarator.name, &declarator.init) else {
            continue;
        };
        if !matches!(initializer, Expr::Fn(_) | Expr::Arrow(_)) {
            continue;
        }
        let before_declaration = declaration.kind == VarDeclKind::Const
            && has_compiler_hint(comments, comments_start, statement_start, "NO_SIDE_EFFECTS");
        let before_initializer = has_compiler_hint(
            comments,
            span_end(binding.id.span),
            span_start(initializer.span()),
            "NO_SIDE_EFFECTS",
        );
        if before_declaration || before_initializer {
            bindings.insert(ident_to_string(&binding.id), true);
        }
    }
    bindings
}

fn parse_module_dependencies_sync(
    path: &Path,
    source: &str,
    hooks: &JavascriptParserHookSet,
) -> Result<ParsedModule> {
    let allocator = Allocator::new();
    let mut comments = Comments::new_in(&allocator);
    let module = swc_experimental_ecma_parser::with_file_parser(
        &allocator,
        source,
        syntax_for_path(path),
        EsVersion::EsNext,
        Some(&mut comments),
        |parser| parser.parse_module(),
    )
    .map_err(|error| {
        let diagnostic = error.into_diagnostic();
        Error::Parse {
            path: path.to_path_buf(),
            message: diagnostic.to_string(),
        }
    })?;

    let mut parsed = ParsedModule::default();
    let pure_analysis = hooks
        .requires_pure_analysis
        .then(|| PureAnalysis::new(&comments, &module));
    let parser_context = JavascriptParserContext {
        pure_analysis: pure_analysis.as_ref(),
    };
    hooks.program.call(&parser_context, &module, &mut parsed);
    let mut import_bindings = FxHashMap::default();
    collect_module_decl_dependencies(path, &module, &mut parsed, &mut import_bindings)?;
    collect_import_usages(
        &module,
        &import_bindings,
        &mut parsed.dependencies_block.dependencies,
    );
    collect_dynamic_import_dependencies(path, &module, &mut parsed)?;
    let mut comments_start = 0;
    for item in module.body.iter() {
        let statement = JavascriptParserStatement {
            parser: &parser_context,
            item,
            comments_start,
        };
        hooks.statement.call(&statement, &mut parsed);
        comments_start = span_end(item.span());
    }
    hooks.finish.call(&parser_context, &module, &mut parsed);

    Ok(parsed)
}

fn collect_module_decl_dependencies(
    path: &Path,
    module: &Module<'_>,
    parsed: &mut ParsedModule,
    import_bindings: &mut FxHashMap<String, ImportBinding>,
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
    import_bindings: &FxHashMap<String, ImportBinding>,
    dependencies: &mut Vec<Dependency>,
) {
    if import_bindings.is_empty() {
        return;
    }

    let mut visitor = ImportUsageVisitor {
        imports: import_bindings,
        dependencies: Vec::new(),
        scopes: vec![FxHashSet::default()],
    };
    module.visit_with(&mut visitor);
    dependencies.extend(visitor.dependencies);
}

struct ImportUsageVisitor<'imports> {
    imports: &'imports FxHashMap<String, ImportBinding>,
    dependencies: Vec<Dependency>,
    scopes: Vec<FxHashSet<String>>,
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
        self.scopes.push(FxHashSet::default());
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

impl BindingCollector for FxHashSet<String> {
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

#[cfg(test)]
mod tests {
    use std::{
        path::Path,
        sync::{Arc, Mutex},
    };

    use super::{
        JavascriptParserHookSet, parse_module_dependencies_with_hooks, source_is_side_effect_free,
    };

    fn is_pure(source: &str) -> bool {
        source_is_side_effect_free(Path::new("module.js"), source)
    }

    #[test]
    fn parser_hooks_share_one_parse_result_in_phase_order() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut hooks = JavascriptParserHookSet::default();
        let program_events = Arc::clone(&events);
        hooks
            .program
            .tap("program", b"program-test/1", move |_, _, result| {
                program_events.lock().unwrap().push("program");
                result.build_meta.side_effect_free = Some(true);
            });
        let statement_events = Arc::clone(&events);
        hooks
            .statement
            .tap("statement", b"statement-test/1", move |_, _| {
                statement_events.lock().unwrap().push("statement");
            });
        let finish_events = Arc::clone(&events);
        hooks
            .finish
            .tap("finish", b"finish-test/1", move |_, _, result| {
                finish_events.lock().unwrap().push("finish");
                assert_eq!(result.build_meta.side_effect_free, Some(true));
            });

        let parsed = parse_module_dependencies_with_hooks(
            Path::new("module.js"),
            "const value = 1; export { value };",
            &hooks,
        )
        .unwrap();

        assert_eq!(parsed.build_meta.side_effect_free, Some(true));
        assert_eq!(
            *events.lock().unwrap(),
            ["program", "statement", "statement", "finish"]
        );
    }

    #[test]
    fn parser_hook_plan_is_part_of_the_module_build_cache_identity() {
        let baseline = JavascriptParserHookSet::default().cache_fingerprint();
        let mut hooks = JavascriptParserHookSet::default();
        hooks.program.tap("analysis", b"analysis/1", |_, _, _| {});
        assert_ne!(hooks.cache_fingerprint(), baseline);

        let first_version = hooks.cache_fingerprint();
        hooks = JavascriptParserHookSet::default();
        hooks.program.tap("analysis", b"analysis/2", |_, _, _| {});
        assert_ne!(hooks.cache_fingerprint(), first_version);

        let program_phase = hooks.cache_fingerprint();
        hooks = JavascriptParserHookSet::default();
        hooks.statement.tap("analysis", b"analysis/2", |_, _| {});
        assert_ne!(hooks.cache_fingerprint(), program_phase);

        let before_pure_analysis = hooks.cache_fingerprint();
        hooks.require_pure_analysis();
        assert_ne!(hooks.cache_fingerprint(), before_pure_analysis);
    }

    #[test]
    fn pure_annotations_cover_calls_construction_and_tagged_templates() {
        assert!(is_pure("const value = /*#__PURE__*/ factory();"));
        assert!(is_pure("const value = /*#__PURE__*/ (factory());"));
        assert!(is_pure("const value = /*@__PURE__*/ new Factory();"));
        assert!(is_pure("const value = /*#__PURE__*/ tag`value ${1}`;"));

        assert!(!is_pure(
            "const value = /*#__PURE__*/ factory(sideEffect());"
        ));
        assert!(!is_pure(
            "const value = /*#__PURE__*/ tag`value ${sideEffect()}`;"
        ));
        assert!(!is_pure("const value = `/*#__PURE__*/ ${sideEffect()}`;"));
        assert!(!is_pure(
            "const value = /* explanation\n#__PURE__*/ factory();"
        ));
    }

    #[test]
    fn no_side_effects_annotations_apply_only_to_function_bindings() {
        assert!(is_pure(
            "/*#__NO_SIDE_EFFECTS__*/ const fn1 = () => 1; fn1();"
        ));
        assert!(is_pure(
            "let fn2 = /*@__NO_SIDE_EFFECTS__*/ () => 2; fn2();"
        ));
        assert!(is_pure(
            "/*#__NO_SIDE_EFFECTS__*/ const fn3 = () => 3; { const fn3 = () => 4; } fn3();"
        ));
        assert!(is_pure(
            "{ /*#__NO_SIDE_EFFECTS__*/ const nested = () => 4; nested(); }"
        ));
        assert!(is_pure(
            "/*#__NO_SIDE_EFFECTS__*/ function fn4() {} { var fn4; fn4(); }"
        ));
        assert!(is_pure(
            "{ var nestedVar = /*@__NO_SIDE_EFFECTS__*/ () => 5; nestedVar(); }"
        ));

        assert!(!is_pure(
            "/*#__NO_SIDE_EFFECTS__*/ const value = 1; value();"
        ));
        assert!(!is_pure(
            "/*#__NO_SIDE_EFFECTS__*/ let fn5 = () => 5; fn5();"
        ));
        assert!(!is_pure(
            "/*#__NO_SIDE_EFFECTS__*/ const fn6 = () => 6; { const fn6 = () => 7; fn6(); }"
        ));
        assert!(!is_pure(
            "/* explanation\n#__NO_SIDE_EFFECTS__*/ const fn7 = () => 7; fn7();"
        ));
        assert!(!is_pure(
            "const object = { method() { var leaked = /*#__NO_SIDE_EFFECTS__*/ () => 1; } }; const leaked = () => sideEffect(); leaked();"
        ));
    }

    #[test]
    fn pure_analysis_models_values_without_invoking_user_code() {
        assert!(is_pure(
            "const value = [1, { key: `value ${2}` }, flag ? 3 : 4];"
        ));
        assert!(is_pure("const value = typeof input === 'undefined';"));
        assert!(is_pure("const value = 1 + 2;"));
        assert!(is_pure("const value = 1n / 1n;"));
        assert!(is_pure("const value = 1n / (1n);"));
        assert!(is_pure("const value = 2n ** 3n;"));
        assert!(is_pure("const value = 2n ** (3n);"));

        assert!(!is_pure("const value = [...items];"));
        assert!(!is_pure("const value = { ...items };"));
        assert!(!is_pure("const value = object.property;"));
        assert!(!is_pure("const value = object + 1;"));
        assert!(!is_pure("const value = 1n + 1;"));
        assert!(!is_pure("const value = 1n / 0n;"));
        assert!(!is_pure("const value = 4n % 2n;"));
        assert!(!is_pure("const value = 2n ** -1n;"));
    }

    #[test]
    fn class_analysis_only_checks_definition_time_effects() {
        assert!(is_pure(
            "class Deferred { field = sideEffect(); method() { sideEffect(); } }"
        ));
        assert!(!is_pure("class Derived extends Base { constructor() {} }"));
        assert!(!is_pure("class Immediate { static field = sideEffect(); }"));
        assert!(!is_pure("class Immediate { static { sideEffect(); } }"));
        assert!(!is_pure("class Computed { [sideEffect()]() {} }"));
    }

    #[test]
    fn top_level_control_flow_is_pure_when_its_executed_parts_are_pure() {
        assert!(is_pure("if (flag) { const value = 1; }"));
        assert!(is_pure("while (false) { const value = 1; }"));
        assert!(is_pure("for (let index = 0; index !== 0; index) {}"));
        assert!(is_pure("switch (value) { case sideEffect(): }"));

        assert!(!is_pure("if (sideEffect()) {}"));
        assert!(!is_pure("if (flag) { sideEffect(); }"));
        assert!(!is_pure("for (sideEffect(); false;) {}"));
    }
}
