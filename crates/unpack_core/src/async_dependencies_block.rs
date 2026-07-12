use serde::{Deserialize, Serialize};

use crate::Dependency;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AsyncDependenciesBlock {
    dependencies: Vec<Dependency>,
}

impl AsyncDependenciesBlock {
    pub fn new(dependencies: Vec<Dependency>) -> Self {
        Self { dependencies }
    }

    pub fn dependencies(&self) -> &[Dependency] {
        &self.dependencies
    }
}
