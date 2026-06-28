use std::path::PathBuf;

use crate::{AsyncDependenciesBlock, Dependency, Error, ExportsInfo};

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
    blocks: Vec<AsyncDependenciesBlock>,
    presentational_dependencies: Vec<Dependency>,
    exports_info: ExportsInfo,
    source: String,
    source_len: usize,
    build_error: Option<Error>,
}

impl Module {
    pub(crate) fn new(id: ModuleId, identity: ModuleIdentity) -> Self {
        Self {
            id,
            identity,
            dependencies: Vec::new(),
            blocks: Vec::new(),
            presentational_dependencies: Vec::new(),
            exports_info: ExportsInfo::default(),
            source: String::new(),
            source_len: 0,
            build_error: None,
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

    pub fn blocks(&self) -> &[AsyncDependenciesBlock] {
        &self.blocks
    }

    pub fn presentational_dependencies(&self) -> &[Dependency] {
        &self.presentational_dependencies
    }

    pub fn exports_info(&self) -> &ExportsInfo {
        &self.exports_info
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn source_len(&self) -> usize {
        self.source_len
    }

    pub fn build_error(&self) -> Option<&Error> {
        self.build_error.as_ref()
    }

    pub(crate) fn finish_build(
        &mut self,
        dependencies: Vec<Dependency>,
        blocks: Vec<AsyncDependenciesBlock>,
        presentational_dependencies: Vec<Dependency>,
        source: String,
    ) {
        self.exports_info = ExportsInfo::from_dependencies(&dependencies);
        self.source_len = source.len();
        self.source = source;
        self.dependencies = dependencies;
        self.blocks = blocks;
        self.presentational_dependencies = presentational_dependencies;
        self.build_error = None;
    }

    pub(crate) fn fail_build(&mut self, error: Error, source: String) {
        self.exports_info = ExportsInfo::default();
        self.source_len = source.len();
        self.source = source;
        self.dependencies.clear();
        self.blocks.clear();
        self.presentational_dependencies.clear();
        self.build_error = Some(error);
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
