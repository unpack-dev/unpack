// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/HarmonyImportSpecifierDependency.js

use serde::{Deserialize, Serialize};

use super::ModuleDependency;
use crate::SourceRange;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
