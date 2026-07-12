// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/DependenciesBlock.js

use serde::{Deserialize, Serialize};

use crate::{AsyncDependenciesBlock, Dependency};

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DependenciesBlock {
    pub(crate) dependencies: Vec<Dependency>,
    pub(crate) blocks: Vec<AsyncDependenciesBlock>,
}

impl DependenciesBlock {
    pub fn new(dependencies: Vec<Dependency>, blocks: Vec<AsyncDependenciesBlock>) -> Self {
        Self {
            dependencies,
            blocks,
        }
    }

    pub fn dependencies(&self) -> &[Dependency] {
        &self.dependencies
    }

    pub fn blocks(&self) -> &[AsyncDependenciesBlock] {
        &self.blocks
    }
}
