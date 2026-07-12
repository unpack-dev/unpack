use serde::{Deserialize, Serialize};

use super::ModuleDependency;
use crate::SourceRange;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
