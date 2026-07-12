// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/ModuleGraphConnection.js

use crate::{AsyncDependenciesBlockIndex, Dependency, DependencyIndex, ModuleHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleGraphConnectionHandle(usize);

impl ModuleGraphConnectionHandle {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleGraphConnection {
    pub handle: ModuleGraphConnectionHandle,
    pub origin_module: Option<ModuleHandle>,
    pub origin_block: Option<AsyncDependenciesBlockIndex>,
    pub origin_dependency_index: Option<DependencyIndex>,
    pub dependency: Dependency,
    pub module: ModuleHandle,
    pub(crate) state: ModuleGraphConnectionState,
}

impl ModuleGraphConnection {
    pub fn state(&self) -> ModuleGraphConnectionState {
        self.state
    }

    pub fn is_active(&self) -> bool {
        self.state != ModuleGraphConnectionState::Inactive
    }

    pub(crate) fn set_state(&mut self, state: ModuleGraphConnectionState) {
        self.state = state;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleGraphConnectionState {
    Active,
    Inactive,
    Circular,
}
