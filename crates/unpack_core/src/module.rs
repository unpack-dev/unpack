// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/Module.js

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
pub struct ModuleHandle(usize);

impl ModuleHandle {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

impl crate::index_vec::Idx for ModuleHandle {
    fn from_usize(index: usize) -> Self {
        Self(index)
    }

    fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    handle: ModuleHandle,
    identity: ModuleIdentity,
    built_content: Arc<BuiltModuleContent>,
    exports_info: ExportsInfo,
    build_error: Option<Error>,
    harmony: bool,
    factory_side_effect_free: Option<bool>,
    build_side_effect_free: Option<bool>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BuiltModuleContent {
    parsed: ParsedModule,
    source: String,
    binary_source: Option<Vec<u8>>,
    source_hash: u64,
    code_generation_local_input_digest: u64,
}

impl BuiltModuleContent {
    pub(crate) fn new(parsed: ParsedModule, source: String) -> Self {
        let source_hash = stable_hash(&source);
        Self::from_persistent_parts_with_binary(parsed, source, None, source_hash)
    }

    pub(crate) fn new_binary(parsed: ParsedModule, source: String, binary_source: Vec<u8>) -> Self {
        let source_hash = stable_hash(&binary_source);
        Self::from_persistent_parts_with_binary(parsed, source, Some(binary_source), source_hash)
    }

    #[cfg(test)]
    pub(crate) fn from_persistent_parts(
        parsed: ParsedModule,
        source: String,
        source_hash: u64,
    ) -> Self {
        Self::from_persistent_parts_with_binary(parsed, source, None, source_hash)
    }

    pub(crate) fn from_persistent_parts_with_binary(
        parsed: ParsedModule,
        source: String,
        binary_source: Option<Vec<u8>>,
        source_hash: u64,
    ) -> Self {
        let mut hasher = StableHasher::default();
        hasher.write(b"unpack/code-generation/local-inputs/1");
        parsed.dependencies_block.hash(&mut hasher);
        parsed.presentational_dependencies.hash(&mut hasher);
        parsed.data.hash(&mut hasher);
        let code_generation_local_input_digest = hasher.finish();
        Self {
            parsed,
            source,
            binary_source,
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

    pub(crate) fn binary_source(&self) -> Option<&[u8]> {
        self.binary_source.as_deref()
    }

    pub(crate) fn source_hash(&self) -> u64 {
        self.source_hash
    }

    pub(crate) fn code_generation_local_input_digest(&self) -> u64 {
        self.code_generation_local_input_digest
    }
}

impl Module {
    pub(crate) fn new(handle: ModuleHandle, identity: ModuleIdentity) -> Self {
        Self {
            handle,
            identity,
            built_content: Arc::new(BuiltModuleContent::new(
                ParsedModule::default(),
                String::new(),
            )),
            exports_info: ExportsInfo::default(),
            build_error: None,
            harmony: false,
            factory_side_effect_free: None,
            build_side_effect_free: None,
        }
    }

    pub(crate) fn from_unsafe_cache(handle: ModuleHandle, cached: &Self) -> Self {
        let mut module = cached.clone();
        module.handle = handle;
        module
    }

    pub fn handle(&self) -> ModuleHandle {
        self.handle
    }

    pub fn identity(&self) -> &ModuleIdentity {
        &self.identity
    }

    pub fn dependencies(&self) -> &[Dependency] {
        self.built_content.parsed.dependencies_block.dependencies()
    }

    pub fn blocks(&self) -> &[AsyncDependenciesBlock] {
        self.built_content.parsed.dependencies_block.blocks()
    }

    pub fn presentational_dependencies(&self) -> &[Dependency] {
        &self.built_content.parsed.presentational_dependencies
    }

    pub fn exports_info(&self) -> &ExportsInfo {
        &self.exports_info
    }

    pub(crate) fn exports_info_mut(&mut self) -> &mut ExportsInfo {
        &mut self.exports_info
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

    pub(crate) fn source_bytes(&self) -> &[u8] {
        self.built_content
            .binary_source()
            .unwrap_or_else(|| self.source().as_bytes())
    }

    pub(crate) fn code_generation_local_input_digest(&self) -> u64 {
        self.built_content.code_generation_local_input_digest()
    }

    pub(crate) fn parsed_data(&self) -> &crate::parser::ParsedModuleData {
        &self.built_content.parsed().data
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

    pub(crate) fn is_side_effect_free(&self) -> bool {
        self.factory_side_effect_free
            .or(self.build_side_effect_free)
            .unwrap_or(false)
    }

    pub(crate) fn set_factory_side_effect_free(&mut self, side_effect_free: Option<bool>) {
        self.factory_side_effect_free = side_effect_free;
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
                dependencies_block: crate::DependenciesBlock::new(dependencies, blocks),
                presentational_dependencies,
                data: crate::parser::ParsedModuleData::JavaScript,
                build_meta: Default::default(),
            },
            source,
            source_hash,
        )));
    }

    pub(crate) fn finish_build_content(&mut self, content: Arc<BuiltModuleContent>) {
        self.exports_info = ExportsInfo::default();
        self.build_side_effect_free = content.parsed.build_meta.side_effect_free;
        self.harmony = content
            .parsed
            .dependencies_block
            .dependencies()
            .iter()
            .chain(&content.parsed.presentational_dependencies)
            .any(Dependency::is_harmony_dependency);
        self.built_content = content;
        self.build_error = None;
    }

    pub(crate) fn analyze_provided_exports(&mut self) {
        self.exports_info = match self.parsed_data() {
            crate::parser::ParsedModuleData::Json(value) => {
                let mut names = vec!["default".to_string()];
                if let serde_json::Value::Object(object) = value {
                    names.extend(object.keys().cloned());
                }
                ExportsInfo::from_names(names)
            }
            crate::parser::ParsedModuleData::Asset { .. } => {
                ExportsInfo::from_names(["default".to_string()])
            }
            crate::parser::ParsedModuleData::JavaScript => ExportsInfo::from_dependencies(
                self.built_content.parsed.dependencies_block.dependencies(),
            ),
        };
    }

    pub(crate) fn fail_build(&mut self, error: Error, source: String) {
        self.exports_info = ExportsInfo::default();
        self.built_content = Arc::new(BuiltModuleContent::new(ParsedModule::default(), source));
        self.build_error = Some(error);
        self.build_side_effect_free = None;
        self.harmony = false;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ModuleType {
    JavaScriptAuto,
    Json,
    Asset,
    AssetResource,
    AssetInline,
    AssetSource,
}
