// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/Dependency.js

use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

use crate::dependencies::{
    ConstDependency, ConstDependencyTemplate, EntryDependency, HarmonyExportExpressionDependency,
    HarmonyExportExpressionDependencyTemplate, HarmonyExportHeaderDependency,
    HarmonyExportHeaderDependencyTemplate, HarmonyExportImportedSpecifierDependency,
    HarmonyExportImportedSpecifierDependencyTemplate, HarmonyExportSpecifierDependency,
    HarmonyExportSpecifierDependencyTemplate, HarmonyImportSideEffectDependency,
    HarmonyImportSideEffectDependencyTemplate, HarmonyImportSpecifierDependency,
    HarmonyImportSpecifierDependencyTemplate, ImportDependency, ImportDependencyTemplate,
    ModuleDependency, NullDependency, NullDependencyTemplate,
};
use crate::dependency_template::{DependencyTemplateContext, apply_dependency_template};
use crate::{ExportsInfo, cache_hash::StableHasher};

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
        self.module_dependency()
            .map(|dependency| dependency.request.as_str())
    }

    pub fn resource_identifier(&self) -> Option<String> {
        self.module_dependency()
            .map(ModuleDependency::resource_identifier)
    }

    pub fn source_order(&self) -> Option<usize> {
        self.module_dependency()
            .and_then(|dependency| dependency.source_order)
    }

    fn module_dependency(&self) -> Option<&ModuleDependency> {
        match self {
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

    pub(crate) fn can_concatenate(&self) -> bool {
        matches!(
            self,
            Self::HarmonyImportSideEffect(_)
                | Self::HarmonyImportSpecifier(_)
                | Self::HarmonyExportImportedSpecifier(_)
        )
    }

    pub(crate) fn apply_template(
        &self,
        source: &mut rspack_sources::ReplaceSource,
        context: &mut DependencyTemplateContext<'_>,
    ) -> Result<(), crate::Error> {
        match self {
            Self::Entry(_) => Ok(()),
            Self::HarmonyImportSideEffect(dependency) => apply_dependency_template(
                &HarmonyImportSideEffectDependencyTemplate,
                dependency,
                source,
                context,
            ),
            Self::HarmonyImportSpecifier(dependency) => apply_dependency_template(
                &HarmonyImportSpecifierDependencyTemplate,
                dependency,
                source,
                context,
            ),
            Self::HarmonyExportHeader(dependency) => apply_dependency_template(
                &HarmonyExportHeaderDependencyTemplate,
                dependency,
                source,
                context,
            ),
            Self::HarmonyExportSpecifier(dependency) => apply_dependency_template(
                &HarmonyExportSpecifierDependencyTemplate,
                dependency,
                source,
                context,
            ),
            Self::HarmonyExportExpression(dependency) => apply_dependency_template(
                &HarmonyExportExpressionDependencyTemplate,
                dependency,
                source,
                context,
            ),
            Self::HarmonyExportImportedSpecifier(dependency) => apply_dependency_template(
                &HarmonyExportImportedSpecifierDependencyTemplate,
                dependency,
                source,
                context,
            ),
            Self::Null(dependency) => {
                apply_dependency_template(&NullDependencyTemplate, dependency, source, context)
            }
            Self::Const(dependency) => {
                apply_dependency_template(&ConstDependencyTemplate, dependency, source, context)
            }
            Self::Import(dependency) => {
                apply_dependency_template(&ImportDependencyTemplate, dependency, source, context)
            }
        }
    }

    pub(crate) fn update_code_generation_hash(
        &self,
        exports_info: &ExportsInfo,
        hasher: &mut StableHasher,
    ) {
        match self {
            Self::HarmonyExportSpecifier(dependency) => {
                hasher.write_u8(0);
                exports_info.get_used_name(&dependency.name).hash(hasher);
            }
            Self::HarmonyExportExpression(_) => {
                hasher.write_u8(1);
                exports_info.get_used_name("default").hash(hasher);
            }
            Self::HarmonyExportImportedSpecifier(dependency) => {
                hasher.write_u8(2);
                dependency
                    .name
                    .as_deref()
                    .and_then(|name| exports_info.get_used_name(name))
                    .hash(hasher);
            }
            _ => {}
        }
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
