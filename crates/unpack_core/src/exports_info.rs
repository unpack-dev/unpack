use std::collections::BTreeSet;

use crate::Dependency;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExportsInfo {
    provided_exports: BTreeSet<String>,
}

impl ExportsInfo {
    pub(crate) fn from_dependencies(dependencies: &[Dependency]) -> Self {
        let mut exports_info = Self::default();
        for dependency in dependencies {
            match dependency {
                Dependency::HarmonyExportSpecifier(dep) => {
                    exports_info.add_provided_export(dep.name.clone());
                }
                Dependency::HarmonyExportExpression(_) => {
                    exports_info.add_provided_export("default");
                }
                Dependency::HarmonyExportImportedSpecifier(dep) => {
                    if let Some(name) = &dep.name {
                        exports_info.add_provided_export(name.clone());
                    }
                }
                Dependency::Entry(_)
                | Dependency::HarmonyImportSideEffect(_)
                | Dependency::HarmonyImportSpecifier(_)
                | Dependency::HarmonyExportHeader(_)
                | Dependency::Null(_)
                | Dependency::Const(_)
                | Dependency::Import(_) => {}
            }
        }
        exports_info
    }

    pub fn provided_exports(&self) -> impl Iterator<Item = &str> {
        self.provided_exports.iter().map(String::as_str)
    }

    pub fn is_export_provided(&self, name: &str) -> bool {
        self.provided_exports.contains(name)
    }

    pub fn get_used_name<'a>(&self, name: &'a str) -> Option<&'a str> {
        Some(name)
    }

    fn add_provided_export(&mut self, name: impl Into<String>) {
        self.provided_exports.insert(name.into());
    }
}
