use serde::{Deserialize, Serialize};

use crate::dependencies::{
    ConstDependency, EntryDependency, HarmonyExportExpressionDependency,
    HarmonyExportHeaderDependency, HarmonyExportImportedSpecifierDependency,
    HarmonyExportSpecifierDependency, HarmonyImportSideEffectDependency,
    HarmonyImportSpecifierDependency, ImportDependency, NullDependency,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

    pub(crate) fn is_harmony_dependency(&self) -> bool {
        matches!(
            self,
            Self::HarmonyImportSideEffect(_)
                | Self::HarmonyImportSpecifier(_)
                | Self::HarmonyExportHeader(_)
                | Self::HarmonyExportSpecifier(_)
                | Self::HarmonyExportExpression(_)
                | Self::HarmonyExportImportedSpecifier(_)
        )
    }

    pub(crate) fn source_ranges(&self) -> Vec<SourceRange> {
        let mut ranges = Vec::new();
        match self {
            Self::Entry(_) | Self::HarmonyExportSpecifier(_) | Self::Null(_) => {}
            Self::HarmonyImportSideEffect(dependency) => ranges.extend(dependency.module.range),
            Self::HarmonyImportSpecifier(dependency) => {
                ranges.extend(dependency.module.range);
                ranges.push(dependency.usage_range);
            }
            Self::HarmonyExportHeader(dependency) => {
                ranges.push(dependency.statement_range);
                ranges.extend(dependency.declaration_range);
            }
            Self::HarmonyExportExpression(dependency) => {
                ranges.push(dependency.statement_range);
                ranges.push(dependency.range);
            }
            Self::HarmonyExportImportedSpecifier(dependency) => {
                ranges.extend(dependency.module.range);
            }
            Self::Const(dependency) => ranges.push(dependency.range),
            Self::Import(dependency) => ranges.extend(dependency.module.range),
        }
        ranges
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DependencyKind {
    Entry,
    StaticImport,
    StaticExport,
    DynamicImport,
    Presentational,
}
