// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/ImportDependency.js

use serde::{Deserialize, Serialize};

use super::ModuleDependency;
use crate::SourceRange;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImportDependency {
    pub module: ModuleDependency,
}

impl ImportDependency {
    pub fn new(
        request: impl Into<String>,
        range: SourceRange,
        source_order: Option<usize>,
    ) -> Self {
        let mut module = ModuleDependency::new(request, source_order);
        module.range = Some(range);
        Self { module }
    }

    pub fn range(&self) -> SourceRange {
        self.module
            .range
            .expect("import dependency should have range")
    }
}
