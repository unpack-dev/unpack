// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/HarmonyExportImportedSpecifierDependency.js

use serde::{Deserialize, Serialize};

use super::ModuleDependency;
use crate::SourceRange;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
