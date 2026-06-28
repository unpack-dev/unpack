use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceRange {
    pub start: u32,
    pub end: u32,
}

impl SourceRange {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub const fn insert(offset: u32) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AsyncDependenciesBlock {
    dependencies: Vec<Dependency>,
}

impl AsyncDependenciesBlock {
    pub fn new(dependencies: Vec<Dependency>) -> Self {
        Self { dependencies }
    }

    pub fn dependencies(&self) -> &[Dependency] {
        &self.dependencies
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Dependency {
    Entry(EntryDependency),
    HarmonyImportSideEffect(HarmonyImportSideEffectDependency),
    HarmonyImportSpecifier(HarmonyImportSpecifierDependency),
    HarmonyExportHeader(HarmonyExportHeaderDependency),
    HarmonyExportSpecifier(HarmonyExportSpecifierDependency),
    HarmonyExportExpression(HarmonyExportExpressionDependency),
    HarmonyExportImportedSpecifier(HarmonyExportImportedSpecifierDependency),
    Null(NullDependency),
    Const(ConstDependency),
    Import(ImportDependency),
}

impl Dependency {
    pub fn new(kind: DependencyKind, request: impl Into<String>) -> Self {
        let request = request.into();
        match kind {
            DependencyKind::Entry => Self::Entry(EntryDependency::new(request)),
            DependencyKind::StaticImport => Self::HarmonyImportSideEffect(
                HarmonyImportSideEffectDependency::new(request, 0, None),
            ),
            DependencyKind::StaticExport => {
                Self::HarmonyExportImportedSpecifier(HarmonyExportImportedSpecifierDependency::new(
                    request,
                    0,
                    Vec::new(),
                    None,
                    false,
                    None,
                ))
            }
            DependencyKind::DynamicImport => {
                Self::Import(ImportDependency::new(request, SourceRange::insert(0), None))
            }
            DependencyKind::Presentational => {
                Self::Const(ConstDependency::new("", SourceRange::insert(0)))
            }
        }
    }

    pub fn kind(&self) -> DependencyKind {
        match self {
            Self::Entry(_) => DependencyKind::Entry,
            Self::HarmonyImportSideEffect(_) | Self::HarmonyImportSpecifier(_) => {
                DependencyKind::StaticImport
            }
            Self::HarmonyExportImportedSpecifier(_) => DependencyKind::StaticExport,
            Self::Import(_) => DependencyKind::DynamicImport,
            Self::HarmonyExportHeader(_)
            | Self::HarmonyExportSpecifier(_)
            | Self::HarmonyExportExpression(_)
            | Self::Null(_)
            | Self::Const(_) => DependencyKind::Presentational,
        }
    }

    pub fn request(&self) -> Option<&str> {
        match self {
            Self::Entry(dep) => Some(&dep.module.request),
            Self::HarmonyImportSideEffect(dep) => Some(&dep.module.request),
            Self::HarmonyImportSpecifier(dep) => Some(&dep.module.request),
            Self::HarmonyExportImportedSpecifier(dep) => Some(&dep.module.request),
            Self::Import(dep) => Some(&dep.module.request),
            Self::HarmonyExportHeader(_)
            | Self::HarmonyExportSpecifier(_)
            | Self::HarmonyExportExpression(_)
            | Self::Null(_)
            | Self::Const(_) => None,
        }
    }

    pub fn resource_identifier(&self) -> Option<String> {
        let module = match self {
            Self::Entry(dep) => Some(&dep.module),
            Self::HarmonyImportSideEffect(dep) => Some(&dep.module),
            Self::HarmonyImportSpecifier(dep) => Some(&dep.module),
            Self::HarmonyExportImportedSpecifier(dep) => Some(&dep.module),
            Self::Import(dep) => Some(&dep.module),
            Self::HarmonyExportHeader(_)
            | Self::HarmonyExportSpecifier(_)
            | Self::HarmonyExportExpression(_)
            | Self::Null(_)
            | Self::Const(_) => None,
        }?;

        Some(module.resource_identifier())
    }

    pub fn source_order(&self) -> Option<usize> {
        match self {
            Self::Entry(dep) => dep.module.source_order,
            Self::HarmonyImportSideEffect(dep) => dep.module.source_order,
            Self::HarmonyImportSpecifier(dep) => dep.module.source_order,
            Self::HarmonyExportImportedSpecifier(dep) => dep.module.source_order,
            Self::Import(dep) => dep.module.source_order,
            Self::HarmonyExportHeader(_)
            | Self::HarmonyExportSpecifier(_)
            | Self::HarmonyExportExpression(_)
            | Self::Null(_)
            | Self::Const(_) => None,
        }
    }

    pub fn is_module_dependency(&self) -> bool {
        self.resource_identifier().is_some()
    }

    pub fn is_static_module_dependency(&self) -> bool {
        matches!(
            self,
            Self::Entry(_)
                | Self::HarmonyImportSideEffect(_)
                | Self::HarmonyImportSpecifier(_)
                | Self::HarmonyExportImportedSpecifier(_)
        )
    }

    pub fn is_import_dependency(&self) -> bool {
        matches!(self, Self::Import(_))
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ModuleDependency {
    pub request: String,
    pub user_request: String,
    pub source_order: Option<usize>,
    pub range: Option<SourceRange>,
    pub weak: bool,
}

impl ModuleDependency {
    pub fn new(request: impl Into<String>, source_order: Option<usize>) -> Self {
        let request = request.into();
        Self {
            user_request: request.clone(),
            request,
            source_order,
            range: None,
            weak: false,
        }
    }

    pub fn resource_identifier(&self) -> String {
        format!("context|module{}", self.request)
    }
}

impl fmt::Debug for ModuleDependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModuleDependency")
            .field("request", &self.request)
            .field("source_order", &self.source_order)
            .field("range", &self.range)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntryDependency {
    pub module: ModuleDependency,
}

impl EntryDependency {
    pub fn new(request: impl Into<String>) -> Self {
        Self {
            module: ModuleDependency::new(request, None),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HarmonyImportSideEffectDependency {
    pub module: ModuleDependency,
    pub import_var: Option<String>,
}

impl HarmonyImportSideEffectDependency {
    pub fn new(
        request: impl Into<String>,
        source_order: usize,
        range: Option<SourceRange>,
    ) -> Self {
        let mut module = ModuleDependency::new(request, Some(source_order));
        module.range = range;
        Self {
            module,
            import_var: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HarmonyImportSpecifierDependency {
    pub module: ModuleDependency,
    pub ids: Vec<String>,
    pub name: String,
    pub usage_range: SourceRange,
    pub shorthand: bool,
}

impl HarmonyImportSpecifierDependency {
    pub fn new(
        request: impl Into<String>,
        source_order: usize,
        ids: Vec<String>,
        name: impl Into<String>,
        usage_range: SourceRange,
    ) -> Self {
        Self {
            module: ModuleDependency::new(request, Some(source_order)),
            ids,
            name: name.into(),
            usage_range,
            shorthand: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HarmonyExportHeaderDependency {
    pub declaration_range: Option<SourceRange>,
    pub statement_range: SourceRange,
}

impl HarmonyExportHeaderDependency {
    pub fn new(declaration_range: Option<SourceRange>, statement_range: SourceRange) -> Self {
        Self {
            declaration_range,
            statement_range,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HarmonyExportSpecifierDependency {
    pub id: String,
    pub name: String,
}

impl HarmonyExportSpecifierDependency {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HarmonyExportExpressionDependency {
    pub range: SourceRange,
    pub statement_range: SourceRange,
    pub declaration_id: Option<String>,
}

impl HarmonyExportExpressionDependency {
    pub fn new(
        range: SourceRange,
        statement_range: SourceRange,
        declaration_id: Option<String>,
    ) -> Self {
        Self {
            range,
            statement_range,
            declaration_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HarmonyExportImportedSpecifierDependency {
    pub module: ModuleDependency,
    pub ids: Vec<String>,
    pub name: Option<String>,
    pub is_star: bool,
}

impl HarmonyExportImportedSpecifierDependency {
    pub fn new(
        request: impl Into<String>,
        source_order: usize,
        ids: Vec<String>,
        name: Option<String>,
        is_star: bool,
        range: Option<SourceRange>,
    ) -> Self {
        let mut module = ModuleDependency::new(request, Some(source_order));
        module.range = range;
        Self {
            module,
            ids,
            name,
            is_star,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConstDependency {
    pub expression: String,
    pub range: SourceRange,
}

impl ConstDependency {
    pub fn new(expression: impl Into<String>, range: SourceRange) -> Self {
        Self {
            expression: expression.into(),
            range,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NullDependency;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportDependency {
    pub module: ModuleDependency,
}

impl ImportDependency {
    pub fn new(
        request: impl Into<String>,
        range: SourceRange,
        source_order: Option<usize>,
    ) -> Self {
        let mut module = ModuleDependency::new(request, source_order);
        module.range = Some(range);
        Self { module }
    }

    pub fn range(&self) -> SourceRange {
        self.module
            .range
            .expect("import dependency should have range")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DependencyKind {
    Entry,
    StaticImport,
    StaticExport,
    DynamicImport,
    Presentational,
}
