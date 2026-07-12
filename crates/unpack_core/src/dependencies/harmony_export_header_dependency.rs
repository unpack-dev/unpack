// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/HarmonyExportHeaderDependency.js

use serde::{Deserialize, Serialize};

use crate::SourceRange;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
