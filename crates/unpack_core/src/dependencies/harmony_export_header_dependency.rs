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
