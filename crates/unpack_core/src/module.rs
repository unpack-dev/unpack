use std::{
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::{
    AsyncDependenciesBlock, Dependency, Error, ExportsInfo,
    cache_hash::{StableHasher, stable_hash},
    parser::ParsedModule,
};

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
    built_content: Arc<BuiltModuleContent>,
    exports_info: ExportsInfo,
    build_error: Option<Error>,
    harmony: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BuiltModuleContent {
    parsed: ParsedModule,
    source: String,
    source_hash: u64,
    code_generation_local_input_digest: u64,
}

impl BuiltModuleContent {
    pub(crate) fn new(parsed: ParsedModule, source: String) -> Self {
        let source_hash = stable_hash(&source);
        Self::from_persistent_parts(parsed, source, source_hash)
    }

    pub(crate) fn from_persistent_parts(
        parsed: ParsedModule,
        source: String,
        source_hash: u64,
    ) -> Self {
        let mut hasher = StableHasher::default();
        hasher.write(b"unpack/code-generation/local-inputs/1");
        parsed.dependencies.hash(&mut hasher);
        parsed.blocks.hash(&mut hasher);
        parsed.presentational_dependencies.hash(&mut hasher);
        let code_generation_local_input_digest = hasher.finish();
        Self {
            parsed,
            source,
            source_hash,
            code_generation_local_input_digest,
        }
    }

    pub(crate) fn parsed(&self) -> &ParsedModule {
        &self.parsed
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn source_hash(&self) -> u64 {
        self.source_hash
    }

    pub(crate) fn code_generation_local_input_digest(&self) -> u64 {
        self.code_generation_local_input_digest
    }
}

impl Module {
    pub(crate) fn new(id: ModuleId, identity: ModuleIdentity) -> Self {
        Self {
            id,
            identity,
            built_content: Arc::new(BuiltModuleContent::new(
                ParsedModule::default(),
                String::new(),
            )),
            exports_info: ExportsInfo::default(),
            build_error: None,
            harmony: false,
        }
    }

    pub fn id(&self) -> ModuleId {
        self.id
    }

    pub fn identity(&self) -> &ModuleIdentity {
        &self.identity
    }

    pub fn dependencies(&self) -> &[Dependency] {
        &self.built_content.parsed.dependencies
    }

    pub fn blocks(&self) -> &[AsyncDependenciesBlock] {
        &self.built_content.parsed.blocks
    }

    pub fn presentational_dependencies(&self) -> &[Dependency] {
        &self.built_content.parsed.presentational_dependencies
    }

    pub fn exports_info(&self) -> &ExportsInfo {
        &self.exports_info
    }

    pub fn source(&self) -> &str {
        self.built_content.source()
    }

    pub fn source_len(&self) -> usize {
        self.built_content.source.len()
    }

    pub fn source_hash(&self) -> u64 {
        self.built_content.source_hash()
    }

    pub(crate) fn code_generation_local_input_digest(&self) -> u64 {
        self.built_content.code_generation_local_input_digest()
    }

    #[cfg(test)]
    pub(crate) fn built_content(&self) -> &Arc<BuiltModuleContent> {
        &self.built_content
    }

    pub fn build_error(&self) -> Option<&Error> {
        self.build_error.as_ref()
    }

    pub(crate) fn is_harmony(&self) -> bool {
        self.harmony
    }

    #[cfg(test)]
    pub(crate) fn finish_build(
        &mut self,
        dependencies: Vec<Dependency>,
        blocks: Vec<AsyncDependenciesBlock>,
        presentational_dependencies: Vec<Dependency>,
        source: String,
        source_hash: u64,
    ) {
        self.finish_build_content(Arc::new(BuiltModuleContent::from_persistent_parts(
            ParsedModule {
                dependencies,
                blocks,
                presentational_dependencies,
            },
            source,
            source_hash,
        )));
    }

    pub(crate) fn finish_build_content(&mut self, content: Arc<BuiltModuleContent>) {
        self.exports_info = ExportsInfo::from_dependencies(&content.parsed.dependencies);
        self.harmony = content
            .parsed
            .dependencies
            .iter()
            .chain(&content.parsed.presentational_dependencies)
            .any(Dependency::is_harmony_dependency);
        self.built_content = content;
        self.build_error = None;
    }

    pub(crate) fn fail_build(&mut self, error: Error, source: String) {
        self.exports_info = ExportsInfo::default();
        self.built_content = Arc::new(BuiltModuleContent::new(ParsedModule::default(), source));
        self.build_error = Some(error);
        self.harmony = false;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModuleType {
    JavaScriptAuto,
}
