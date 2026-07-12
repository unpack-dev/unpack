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
