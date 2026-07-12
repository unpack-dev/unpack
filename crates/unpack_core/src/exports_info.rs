use std::collections::BTreeSet;

use crate::Dependency;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExportsInfo {
    provided_exports: Option<BTreeSet<String>>,
    used_exports: Option<BTreeSet<String>>,
    all_exports_used: bool,
}

impl ExportsInfo {
    pub(crate) fn from_dependencies(dependencies: &[Dependency]) -> Self {
        let mut exports_info = Self {
            provided_exports: Some(BTreeSet::new()),
            used_exports: None,
            all_exports_used: false,
        };
        for dependency in dependencies {
            match dependency {
                Dependency::HarmonyExportSpecifier(dep) => {
                    exports_info.add_provided_export(dep.name.clone());
                }
                Dependency::HarmonyExportExpression(_) => {
                    exports_info.add_provided_export("default");
                }
                Dependency::HarmonyExportImportedSpecifier(dep) => {
                    if dep.is_star {
                        exports_info.provided_exports = None;
                    } else if let Some(name) = &dep.name {
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

    pub fn provided_exports(&self) -> Option<impl Iterator<Item = &str>> {
        self.provided_exports
            .as_ref()
            .map(|exports| exports.iter().map(String::as_str))
    }

    pub fn is_export_provided(&self, name: &str) -> Option<bool> {
        self.provided_exports
            .as_ref()
            .map(|exports| exports.contains(name))
    }

    pub fn get_used_name<'a>(&self, name: &'a str) -> Option<&'a str> {
        match &self.used_exports {
            _ if self.all_exports_used => Some(name),
            None => Some(name),
            Some(exports) if exports.contains(name) => Some(name),
            Some(_) => None,
        }
    }

    pub fn used_exports(&self) -> Option<impl Iterator<Item = &str>> {
        self.used_exports
            .as_ref()
            .map(|exports| exports.iter().map(String::as_str))
    }

    pub fn are_all_exports_used(&self) -> bool {
        self.all_exports_used
    }

    pub(crate) fn set_used_exports(&mut self, exports: Option<BTreeSet<String>>) {
        self.used_exports = exports;
        self.all_exports_used = false;
    }

    pub(crate) fn set_all_exports_used(&mut self) {
        self.used_exports = Some(BTreeSet::new());
        self.all_exports_used = true;
    }

    fn add_provided_export(&mut self, name: impl Into<String>) {
        if let Some(exports) = &mut self.provided_exports {
            exports.insert(name.into());
        }
    }
}
