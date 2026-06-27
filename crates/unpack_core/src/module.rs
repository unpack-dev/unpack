use std::path::PathBuf;

use crate::Dependency;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleId(usize);

impl ModuleId {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    id: ModuleId,
    identity: ModuleIdentity,
    dependencies: Vec<Dependency>,
    source_len: usize,
}

impl Module {
    pub(crate) fn new(id: ModuleId, identity: ModuleIdentity) -> Self {
        Self {
            id,
            identity,
            dependencies: Vec::new(),
            source_len: 0,
        }
    }

    pub fn id(&self) -> ModuleId {
        self.id
    }

    pub fn identity(&self) -> &ModuleIdentity {
        &self.identity
    }

    pub fn dependencies(&self) -> &[Dependency] {
        &self.dependencies
    }

    pub fn source_len(&self) -> usize {
        self.source_len
    }

    pub(crate) fn finish_build(&mut self, dependencies: Vec<Dependency>, source_len: usize) {
        self.dependencies = dependencies;
        self.source_len = source_len;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModuleIdentity {
    pub module_type: ModuleType,
    pub resource: PathBuf,
    pub query: Option<String>,
    pub fragment: Option<String>,
    pub layer: Option<String>,
    pub loaders: Vec<String>,
}

impl ModuleIdentity {
    pub fn new(resource: impl Into<PathBuf>) -> Self {
        Self {
            module_type: ModuleType::JavaScriptAuto,
            resource: resource.into(),
            query: None,
            fragment: None,
            layer: None,
            loaders: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModuleType {
    JavaScriptAuto,
}
