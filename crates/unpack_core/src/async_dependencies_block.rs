// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/AsyncDependenciesBlock.js

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
