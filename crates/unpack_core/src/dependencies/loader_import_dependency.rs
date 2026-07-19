// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/LoaderImportDependency.js

use serde::{Deserialize, Serialize};

use super::ModuleDependency;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LoaderImportDependency {
    pub module: ModuleDependency,
}

impl LoaderImportDependency {
    pub fn new(request: impl Into<String>) -> Self {
        let mut module = ModuleDependency::new(request, None);
        module.weak = true;
        Self { module }
    }
}
