// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/HarmonyExportExpressionDependency.js

use serde::{Deserialize, Serialize};

use crate::SourceRange;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
