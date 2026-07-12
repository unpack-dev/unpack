// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/EntryDependency.js

use serde::{Deserialize, Serialize};

use super::ModuleDependency;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntryDependency {
    pub module: ModuleDependency,
}

impl EntryDependency {
    pub fn new(request: impl Into<String>) -> Self {
        Self {
            module: ModuleDependency::new(request, None),
        }
    }
}
