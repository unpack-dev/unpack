use std::path::{Path, PathBuf};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    hash::{Hash, Hasher},
};

use rspack_sources::{ConcatSource, OriginalSource, RawStringSource, ReplaceSource, SourceMap};
use serde::{Deserialize, Serialize};

use crate::{
    AsyncBlockOrigin, Chunk, ChunkGraph, ChunkGroupKind, CompilerOptions, ConstDependency,
    Dependency, Error, ExportsInfo, HarmonyExportExpressionDependency,
    HarmonyExportHeaderDependency, HarmonyExportImportedSpecifierDependency,
    HarmonyExportSpecifierDependency, HarmonyImportSideEffectDependency,
    HarmonyImportSpecifierDependency, ImportDependency, Module, ModuleGraph, ModuleId,
    ModuleIdentity, SourceRange,
    build_cache::{BuildCache, CacheETag, CacheIdentifier, CacheKey},
    cache_hash::StableHasher,
    code_generation_record::{
        CodeGenerationReplacement, CodeGenerationResult, CodeGenerationSource,
    },
    rendered_source::RenderedSource,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub filename: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RuntimeRequirement {
    ModuleFactories,
    ModuleCache,
    Require,
    DefinePropertyGetters,
    HasOwnProperty,
    MakeNamespaceObject,
    EnsureChunk,
    GetChunkFilename,
    RequireChunkLoading,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RuntimeRequirements {
    requirements: BTreeSet<RuntimeRequirement>,
}

impl RuntimeRequirements {
    pub fn insert(&mut self, requirement: RuntimeRequirement) {
        self.requirements.insert(requirement);
    }

    pub fn contains(&self, requirement: RuntimeRequirement) -> bool {
        self.requirements.contains(&requirement)
    }

    pub fn iter(&self) -> impl Iterator<Item = RuntimeRequirement> + '_ {
        self.requirements.iter().copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RuntimeSpec(BTreeSet<String>);

impl RuntimeSpec {
    fn for_chunk(chunk_graph: &ChunkGraph, chunk: &Chunk) -> Self {
        let mut names = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut pending = chunk.groups().to_vec();

        while let Some(group_id) = pending.pop() {
            if !visited.insert(group_id) {
                continue;
            }
            let group = &chunk_graph.chunk_groups()[group_id.index()];
            match group.kind() {
                ChunkGroupKind::Entrypoint { name } => {
                    names.insert(name.clone());
                }
                ChunkGroupKind::Async => pending.extend(group.parents().iter().copied()),
            }
        }

        Self(names)
    }

    #[cfg(test)]
    fn names(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CodeGenerationKey {
    module: ModuleIdentity,
    runtime: RuntimeSpec,
}

impl CodeGenerationKey {
    fn new(module: ModuleIdentity, runtime: RuntimeSpec) -> Self {
        Self { module, runtime }
    }
}

impl CacheKey for CodeGenerationKey {
    fn cache_identifier(&self) -> CacheIdentifier {
        let module = self.module.cache_identifier();
        let mut parts = vec![
            b"code-generation-v1".to_vec(),
            module.as_bytes().to_vec(),
            (self.runtime.0.len() as u64).to_le_bytes().to_vec(),
        ];
        parts.extend(
            self.runtime
                .0
                .iter()
                .map(|runtime| runtime.as_bytes().to_vec()),
        );
        CacheIdentifier::from_parts(parts)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CodeGenerationResults {
    module_render_ids: HashMap<ModuleId, String>,
    module_identities: HashMap<ModuleId, ModuleIdentity>,
    results: HashMap<CodeGenerationKey, CodeGenerationResult>,
}

impl CodeGenerationResults {
    fn key_for(&self, module: ModuleId, runtime: RuntimeSpec) -> Option<CodeGenerationKey> {
        self.module_identities
            .get(&module)
            .cloned()
            .map(|identity| CodeGenerationKey::new(identity, runtime))
    }
}

struct CodeGenerationInput<'a> {
    module: &'a Module,
    module_graph: &'a ModuleGraph,
    chunk_graph: &'a ChunkGraph,
    module_render_ids: &'a HashMap<ModuleId, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderManifest {
    entries: Vec<RenderManifestEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderManifestEntry {
    filename: String,
    render: AssetRenderManifest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AssetRenderManifest {
    InitialChunk {
        modules: Vec<ModuleRenderManifest>,
        chunk_filename_map: String,
        entry_id: String,
        chunk_id: String,
    },
    AsyncChunk {
        modules: Vec<ModuleRenderManifest>,
        chunk_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModuleRenderManifest {
    render_id: String,
    code_generation_key: CodeGenerationKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AssetRenderKey {
    kind: AssetRenderKind,
    chunk_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssetRenderKind {
    Initial,
    Async,
}

impl CacheKey for AssetRenderKey {
    fn cache_identifier(&self) -> CacheIdentifier {
        CacheIdentifier::from_parts([
            match self.kind {
                AssetRenderKind::Initial => b"initial".to_vec(),
                AssetRenderKind::Async => b"async".to_vec(),
            },
            self.chunk_id.as_bytes().to_vec(),
        ])
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InitFragment {
    stage: InitFragmentStage,
    order: usize,
    content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum InitFragmentStage {
    HarmonyImport,
    HarmonyExport,
    HarmonyStarReexport,
}

pub(crate) fn generate_code(
    options: &CompilerOptions,
    build_cache: &BuildCache,
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
) -> CodeGenerationResults {
    let render_context = RenderPathContext::new(options.context.as_path());
    let module_render_ids = module_render_ids(&render_context, module_graph);
    let module_identities = module_graph
        .modules()
        .iter()
        .map(|module| (module.id(), module.identity().clone()))
        .collect::<HashMap<_, _>>();
    let mut results = HashMap::new();
    let cache = build_cache.code_generations::<CodeGenerationKey>();

    for chunk in chunk_graph.chunks() {
        let runtime = RuntimeSpec::for_chunk(chunk_graph, chunk);
        for module_id in chunk_graph.chunk_modules(chunk.id()) {
            let Some(module) = module_graph.module(*module_id) else {
                continue;
            };
            let key = CodeGenerationKey::new(module.identity().clone(), runtime.clone());
            if results.contains_key(&key) {
                continue;
            }
            let input = CodeGenerationInput {
                module,
                module_graph,
                chunk_graph,
                module_render_ids: &module_render_ids,
            };
            let result = if cache.is_enabled() {
                let etag = code_generation_etag(&input);
                if let Some(result) = cache.get(&key, Some(&etag)) {
                    result.as_ref().clone()
                } else {
                    let result = generate_module_code(input);
                    cache.store(key.clone(), Some(etag), result.clone());
                    result
                }
            } else {
                generate_module_code(input)
            };
            results.insert(key, result);
        }
    }

    CodeGenerationResults {
        module_render_ids,
        module_identities,
        results,
    }
}

fn code_generation_etag(input: &CodeGenerationInput<'_>) -> CacheETag {
    let mut hasher = StableHasher::default();
    hasher.write(b"unpack/code-generation/dependency-templates/1");
    input.module.source_hash().hash(&mut hasher);
    input
        .module
        .build_error()
        .map(ToString::to_string)
        .hash(&mut hasher);
    input.module.presentational_dependencies().hash(&mut hasher);
    input.module.dependencies().hash(&mut hasher);
    input.module.blocks().hash(&mut hasher);
    input
        .module
        .exports_info()
        .provided_exports()
        .collect::<Vec<_>>()
        .hash(&mut hasher);
    input.module_render_ids[&input.module.id()].hash(&mut hasher);

    for (dependency_id, dependency) in input.module.dependencies().iter().enumerate() {
        hash_dependency_template_connection(input, dependency, None, dependency_id, &mut hasher);
    }
    for (block_index, block) in input.module.blocks().iter().enumerate() {
        for (dependency_id, dependency) in block.dependencies().iter().enumerate() {
            hash_dependency_template_connection(
                input,
                dependency,
                Some(block_index),
                dependency_id,
                &mut hasher,
            );
        }
    }
    CacheETag::new(hasher.finish().to_le_bytes())
}

fn hash_dependency_template_connection(
    input: &CodeGenerationInput<'_>,
    dependency: &Dependency,
    block_index: Option<usize>,
    dependency_id: usize,
    hasher: &mut StableHasher,
) {
    let target =
        input
            .module_graph
            .module_for_dependency(input.module.id(), block_index, dependency_id);
    match dependency {
        Dependency::HarmonyImportSideEffect(_) => target
            .and_then(|target| input.module_render_ids.get(&target))
            .hash(hasher),
        Dependency::HarmonyImportSpecifier(_) | Dependency::HarmonyExportImportedSpecifier(_) => {
            target.is_some().hash(hasher)
        }
        Dependency::Import(_) => {
            target
                .and_then(|target| input.module_render_ids.get(&target))
                .hash(hasher);
            let chunk_render_id = block_index
                .and_then(|block_index| {
                    input.chunk_graph.block_chunk_group(AsyncBlockOrigin {
                        module: input.module.id(),
                        block_index,
                    })
                })
                .and_then(|group_id| {
                    input.chunk_graph.chunk_groups()[group_id.index()]
                        .chunks()
                        .first()
                        .copied()
                })
                .and_then(|chunk_id| input.chunk_graph.chunk(chunk_id))
                .map(Chunk::render_id);
            chunk_render_id.hash(hasher);
        }
        Dependency::Entry(_)
        | Dependency::HarmonyExportHeader(_)
        | Dependency::HarmonyExportSpecifier(_)
        | Dependency::HarmonyExportExpression(_)
        | Dependency::Null(_)
        | Dependency::Const(_) => {}
    }
}

pub(crate) fn create_render_manifest(
    chunk_graph: &ChunkGraph,
    entries: &[ModuleId],
    code_generation_results: &CodeGenerationResults,
) -> RenderManifest {
    let mut manifest_entries = Vec::new();

    for (entry_index, group_id) in chunk_graph.entrypoints().iter().copied().enumerate() {
        let group = &chunk_graph.chunk_groups()[group_id.index()];
        let Some(chunk_id) = group.chunks().first().copied() else {
            continue;
        };
        let Some(chunk) = chunk_graph.chunk(chunk_id) else {
            continue;
        };
        let Some(entry_module) = entries.get(entry_index).copied() else {
            continue;
        };
        let modules = module_render_manifest(chunk_graph, chunk, code_generation_results);
        manifest_entries.push(RenderManifestEntry {
            filename: chunk.filename().to_string(),
            render: AssetRenderManifest::InitialChunk {
                modules,
                chunk_filename_map: render_chunk_filename_map(chunk_graph),
                entry_id: code_generation_results.module_render_ids[&entry_module].clone(),
                chunk_id: chunk.render_id().to_string(),
            },
        });
    }

    for chunk in chunk_graph.chunks() {
        let is_initial = chunk.groups().iter().any(|group_id| {
            matches!(
                chunk_graph.chunk_groups()[group_id.index()].kind(),
                ChunkGroupKind::Entrypoint { .. }
            )
        });
        if is_initial {
            continue;
        }
        manifest_entries.push(RenderManifestEntry {
            filename: chunk.filename().to_string(),
            render: AssetRenderManifest::AsyncChunk {
                modules: module_render_manifest(chunk_graph, chunk, code_generation_results),
                chunk_id: chunk.render_id().to_string(),
            },
        });
    }

    RenderManifest {
        entries: manifest_entries,
    }
}

pub(crate) fn render_assets(
    options: &CompilerOptions,
    build_cache: &BuildCache,
    manifest: &RenderManifest,
    code_generation_results: &CodeGenerationResults,
) -> Vec<Asset> {
    let mut assets = Vec::new();
    let cache = build_cache.asset_renders::<AssetRenderKey>();
    for entry in &manifest.entries {
        let key = entry.render.cache_key();
        let rendered_source = if cache.is_enabled() {
            let etag = entry.render.cache_etag(code_generation_results);
            if let Some(rendered_source) = cache.get(&key, Some(&etag)) {
                rendered_source.as_ref().clone()
            } else {
                let rendered_source = render_asset(&entry.render, code_generation_results);
                cache.store(key, Some(etag), rendered_source.clone());
                rendered_source
            }
        } else {
            render_asset(&entry.render, code_generation_results)
        };
        assets.extend(emit_asset(
            entry.filename.clone(),
            &rendered_source,
            options.sourcemap,
        ));
    }
    assets
}

impl AssetRenderManifest {
    fn cache_key(&self) -> AssetRenderKey {
        match self {
            Self::InitialChunk { chunk_id, .. } => AssetRenderKey {
                kind: AssetRenderKind::Initial,
                chunk_id: chunk_id.clone(),
            },
            Self::AsyncChunk { chunk_id, .. } => AssetRenderKey {
                kind: AssetRenderKind::Async,
                chunk_id: chunk_id.clone(),
            },
        }
    }

    fn cache_etag(&self, code_generation_results: &CodeGenerationResults) -> CacheETag {
        let mut hasher = StableHasher::default();
        hasher.write(b"unpack/asset-render/hash/1");
        match self {
            Self::InitialChunk {
                modules,
                chunk_filename_map,
                entry_id,
                chunk_id,
            } => {
                hasher.write_u8(0);
                hash_module_render_inputs(modules, code_generation_results, &mut hasher);
                chunk_filename_map.hash(&mut hasher);
                entry_id.hash(&mut hasher);
                chunk_id.hash(&mut hasher);
            }
            Self::AsyncChunk { modules, chunk_id } => {
                hasher.write_u8(1);
                hash_module_render_inputs(modules, code_generation_results, &mut hasher);
                chunk_id.hash(&mut hasher);
            }
        }
        CacheETag::new(hasher.finish().to_le_bytes())
    }
}

fn hash_module_render_inputs(
    modules: &[ModuleRenderManifest],
    code_generation_results: &CodeGenerationResults,
    hasher: &mut StableHasher,
) {
    modules.len().hash(hasher);
    for module in modules {
        module.render_id.hash(hasher);
        module.code_generation_key.hash(hasher);
        match code_generation_results
            .results
            .get(&module.code_generation_key)
        {
            Some(result) => {
                true.hash(hasher);
                result.source().hash(hasher);
            }
            None => false.hash(hasher),
        }
    }
}

fn module_render_manifest(
    chunk_graph: &ChunkGraph,
    chunk: &Chunk,
    code_generation_results: &CodeGenerationResults,
) -> Vec<ModuleRenderManifest> {
    let runtime = RuntimeSpec::for_chunk(chunk_graph, chunk);
    chunk_graph
        .chunk_modules(chunk.id())
        .iter()
        .filter_map(|module_id| {
            let code_generation_key =
                code_generation_results.key_for(*module_id, runtime.clone())?;
            code_generation_results
                .results
                .contains_key(&code_generation_key)
                .then(|| ModuleRenderManifest {
                    render_id: code_generation_results.module_render_ids[module_id].clone(),
                    code_generation_key,
                })
        })
        .collect()
}

fn render_asset(
    manifest: &AssetRenderManifest,
    code_generation_results: &CodeGenerationResults,
) -> RenderedSource {
    let source = match manifest {
        AssetRenderManifest::InitialChunk {
            modules,
            chunk_filename_map,
            entry_id,
            chunk_id,
        } => render_initial_asset(
            modules,
            chunk_filename_map,
            entry_id,
            chunk_id,
            code_generation_results,
        ),
        AssetRenderManifest::AsyncChunk { modules, chunk_id } => {
            render_async_chunk_asset(modules, chunk_id, code_generation_results)
        }
    };
    RenderedSource::new(source)
}

fn emit_asset(filename: String, rendered: &RenderedSource, sourcemap: bool) -> Vec<Asset> {
    let map_filename = format!("{filename}.map");
    let mut source = rendered.source().to_string();
    if sourcemap {
        source.push_str(&format!("\n//# sourceMappingURL={map_filename}\n"));
    }

    let mut assets = Vec::new();
    let mut source_map = if sourcemap {
        rendered
            .source_map()
            .and_then(|source_map| SourceMap::from_json(source_map.to_string()).ok())
    } else {
        None
    };
    if let Some(map) = &mut source_map {
        map.set_file(Some(filename.clone().into()));
    }

    assets.push(Asset { filename, source });

    if let Some(map) = source_map {
        assets.push(Asset {
            filename: map_filename,
            source: map.to_json(),
        });
    }

    assets
}

fn render_initial_asset(
    modules: &[ModuleRenderManifest],
    chunk_filename_map: &str,
    entry_id: &str,
    chunk_id: &str,
    code_generation_results: &CodeGenerationResults,
) -> ConcatSource {
    let modules = render_module_table(modules, code_generation_results);
    let entry_id = json_string(entry_id);
    let chunk_id = json_string(chunk_id);

    let mut source = ConcatSource::default();
    source.add(RawStringSource::from(
        r#""use strict";
var __webpack_modules__ = ({
"#
        .to_string(),
    ));
    source.add(modules);
    source.add(RawStringSource::from(format!(
        r#"
}});

var __webpack_module_cache__ = {{}};
function __webpack_require__(moduleId) {{
  var cachedModule = __webpack_module_cache__[moduleId];
  if (cachedModule !== undefined) {{
    return cachedModule.exports;
  }}
  var module = __webpack_module_cache__[moduleId] = {{
    exports: {{}}
  }};
  __webpack_modules__[moduleId](module, module.exports, __webpack_require__);
  return module.exports;
}}
__webpack_require__.m = __webpack_modules__;
__webpack_require__.d = function(exports, definition) {{
  for(var key in definition) {{
    if(__webpack_require__.o(definition, key) && !__webpack_require__.o(exports, key)) {{
      Object.defineProperty(exports, key, {{ enumerable: true, get: definition[key] }});
    }}
  }}
}};
__webpack_require__.o = function(obj, prop) {{ return Object.prototype.hasOwnProperty.call(obj, prop); }};
__webpack_require__.r = function(exports) {{
  if(typeof Symbol !== "undefined" && Symbol.toStringTag) {{
    Object.defineProperty(exports, Symbol.toStringTag, {{ value: "Module" }});
  }}
  Object.defineProperty(exports, "__esModule", {{ value: true }});
}};
__webpack_require__.u = function(chunkId) {{
  return ({{{chunk_filename_map}}})[chunkId];
}};
__webpack_require__.f = {{}};
__webpack_require__.e = function(chunkId) {{
  return Promise.all(Object.keys(__webpack_require__.f).reduce(function(promises, key) {{
    __webpack_require__.f[key](chunkId, promises);
    return promises;
  }}, []));
}};
var installedChunks = {{
  {chunk_id}: 1
}};
var installChunk = function(chunk) {{
  var moreModules = chunk.modules, chunkIds = chunk.ids, runtime = chunk.runtime;
  for(var moduleId in moreModules) {{
    if(__webpack_require__.o(moreModules, moduleId)) {{
      __webpack_require__.m[moduleId] = moreModules[moduleId];
    }}
  }}
  if(runtime) runtime(__webpack_require__);
  for(var i = 0; i < chunkIds.length; i++) {{
    installedChunks[chunkIds[i]] = 1;
  }}
}};
__webpack_require__.f.require = function(chunkId, promises) {{
  if(!installedChunks[chunkId]) {{
    var installedChunk = require("./" + __webpack_require__.u(chunkId));
    if(!installedChunks[chunkId]) {{
      installChunk(installedChunk);
    }}
  }}
}};
module.exports = __webpack_require__({entry_id});
"#,
    )));
    source
}

fn render_async_chunk_asset(
    modules: &[ModuleRenderManifest],
    chunk_id: &str,
    code_generation_results: &CodeGenerationResults,
) -> ConcatSource {
    let modules = render_module_table(modules, code_generation_results);
    let chunk_id = json_string(chunk_id);
    let mut source = ConcatSource::default();
    source.add(RawStringSource::from(format!(
        r#""use strict";
exports.id = {chunk_id};
exports.ids = [{chunk_id}];
exports.modules = ({{
"#
    )));
    source.add(modules);
    source.add(RawStringSource::from("\n});\n".to_string()));
    source
}

fn render_module_table(
    modules: &[ModuleRenderManifest],
    code_generation_results: &CodeGenerationResults,
) -> ConcatSource {
    let mut source = ConcatSource::default();
    let mut first = true;
    for module in modules {
        let Some(result) = code_generation_results
            .results
            .get(&module.code_generation_key)
        else {
            continue;
        };
        if first {
            first = false;
        } else {
            source.add(RawStringSource::from(",\n".to_string()));
        }
        source.add(RawStringSource::from(format!(
            "{}: ",
            json_string(&module.render_id)
        )));
        source.add(result.source().clone());
    }
    source
}

fn generate_module_code(input: CodeGenerationInput<'_>) -> CodeGenerationResult {
    let CodeGenerationInput {
        module,
        module_graph,
        chunk_graph,
        module_render_ids,
    } = input;
    if let Some(error) = module.build_error() {
        return CodeGenerationResult::new(CodeGenerationSource::Raw {
            source: render_failed_module_factory(error),
        });
    }

    let module_id = module.id();
    let module_render_id = &module_render_ids[&module_id];
    let mut source = ReplaceSource::new(OriginalSource::new(
        module.source(),
        module_render_id.as_str(),
    ));
    let mut init_fragments = Vec::new();

    for dependency in module.presentational_dependencies() {
        apply_dependency_template(
            dependency,
            module_id,
            None,
            None,
            module_graph,
            chunk_graph,
            module.exports_info(),
            module_render_ids,
            &mut source,
            &mut init_fragments,
        );
    }
    for (dependency_id, dependency) in module.dependencies().iter().enumerate() {
        apply_dependency_template(
            dependency,
            module_id,
            None,
            Some(dependency_id),
            module_graph,
            chunk_graph,
            module.exports_info(),
            module_render_ids,
            &mut source,
            &mut init_fragments,
        );
    }
    for (block_index, block) in module.blocks().iter().enumerate() {
        for (dependency_id, dependency) in block.dependencies().iter().enumerate() {
            apply_dependency_template(
                dependency,
                module_id,
                Some(block_index),
                Some(dependency_id),
                module_graph,
                chunk_graph,
                module.exports_info(),
                module_render_ids,
                &mut source,
                &mut init_fragments,
            );
        }
    }

    let init = render_init_fragments(init_fragments);
    CodeGenerationResult::new(CodeGenerationSource::OriginalWithReplacements {
        prefix: format!(
            "((__unused_webpack_module, __webpack_exports__, __webpack_require__) => {{\n\"use strict\";\n__webpack_require__.r(__webpack_exports__);\n{init}"
        ),
        original_source: module.source().to_string(),
        original_name: module_render_id.clone(),
        replacements: source
            .replacements()
            .iter()
            .map(CodeGenerationReplacement::from)
            .collect(),
        suffix: "\n})".to_string(),
    })
}

fn render_failed_module_factory(error: &Error) -> String {
    format!(
        "((__unused_webpack_module, __webpack_exports__, __webpack_require__) => {{\n\"use strict\";\nthrow new Error({});\n}})",
        json_string(&error.to_string())
    )
}

fn render_init_fragments(mut fragments: Vec<InitFragment>) -> String {
    fragments.sort_by_key(|fragment| (fragment.stage, fragment.order));
    fragments
        .into_iter()
        .map(|fragment| fragment.content)
        .collect()
}

fn push_init_fragment(
    init_fragments: &mut Vec<InitFragment>,
    stage: InitFragmentStage,
    content: String,
) {
    init_fragments.push(InitFragment {
        stage,
        order: init_fragments.len(),
        content,
    });
}

#[allow(clippy::too_many_arguments)]
fn apply_dependency_template(
    dependency: &Dependency,
    module_id: ModuleId,
    origin_block: Option<usize>,
    dependency_id: Option<usize>,
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    exports_info: &ExportsInfo,
    module_render_ids: &HashMap<ModuleId, String>,
    source: &mut ReplaceSource,
    init_fragments: &mut Vec<InitFragment>,
) {
    match dependency {
        Dependency::Const(dep) => apply_const_dependency(dep, source),
        Dependency::Null(_) => {}
        Dependency::HarmonyExportHeader(dep) => apply_export_header_dependency(dep, source),
        Dependency::HarmonyImportSideEffect(dep) => apply_harmony_import_side_effect_dependency(
            dep,
            module_id,
            dependency_id,
            module_graph,
            module_render_ids,
            init_fragments,
        ),
        Dependency::HarmonyImportSpecifier(dep) => apply_harmony_import_specifier_dependency(
            dep,
            module_id,
            dependency_id,
            module_graph,
            module_render_ids,
            source,
        ),
        Dependency::HarmonyExportSpecifier(dep) => {
            apply_harmony_export_specifier_dependency(dep, exports_info, init_fragments)
        }
        Dependency::HarmonyExportExpression(dep) => {
            apply_harmony_export_expression_dependency(dep, exports_info, source, init_fragments)
        }
        Dependency::HarmonyExportImportedSpecifier(dep) => {
            apply_harmony_export_imported_specifier_dependency(
                dep,
                module_id,
                dependency_id,
                module_graph,
                exports_info,
                module_render_ids,
                init_fragments,
            )
        }
        Dependency::Import(dep) => apply_import_dependency(
            dep,
            module_id,
            origin_block,
            dependency_id,
            module_graph,
            chunk_graph,
            module_render_ids,
            source,
        ),
        Dependency::Entry(_) => {}
    }
}

fn apply_const_dependency(dep: &ConstDependency, source: &mut ReplaceSource) {
    replace(source, dep.range, dep.expression.clone());
}

fn apply_export_header_dependency(dep: &HarmonyExportHeaderDependency, source: &mut ReplaceSource) {
    let end = dep
        .declaration_range
        .map(|range| range.start)
        .unwrap_or(dep.statement_range.end);
    replace(
        source,
        SourceRange::new(dep.statement_range.start, end),
        String::new(),
    );
}

fn apply_harmony_import_side_effect_dependency(
    dep: &HarmonyImportSideEffectDependency,
    module_id: ModuleId,
    dependency_id: Option<usize>,
    module_graph: &ModuleGraph,
    module_render_ids: &HashMap<ModuleId, String>,
    init_fragments: &mut Vec<InitFragment>,
) {
    let Some(dependency_id) = dependency_id else {
        return;
    };
    let Some(target) = module_graph.module_for_dependency(module_id, None, dependency_id) else {
        return;
    };
    let import_var = import_var(&dep.module.request, dep.module.source_order.unwrap_or(0));
    let target_id = json_string(&module_render_ids[&target]);
    push_init_fragment(
        init_fragments,
        InitFragmentStage::HarmonyImport,
        format!("/* harmony import */ var {import_var} = __webpack_require__({target_id});\n"),
    );
}

fn apply_harmony_import_specifier_dependency(
    dep: &HarmonyImportSpecifierDependency,
    module_id: ModuleId,
    dependency_id: Option<usize>,
    module_graph: &ModuleGraph,
    module_render_ids: &HashMap<ModuleId, String>,
    source: &mut ReplaceSource,
) {
    let Some(dependency_id) = dependency_id else {
        return;
    };
    let Some(_target) = module_graph.module_for_dependency(module_id, None, dependency_id) else {
        return;
    };
    let expression = import_expression(
        &dep.module.request,
        dep.module.source_order.unwrap_or(0),
        &dep.ids,
    );
    let expression = if dep.shorthand {
        format!("{}: {expression}", dep.name)
    } else {
        expression
    };
    replace(source, dep.usage_range, expression);
    let _ = module_render_ids;
}

fn apply_harmony_export_specifier_dependency(
    dep: &HarmonyExportSpecifierDependency,
    exports_info: &ExportsInfo,
    init_fragments: &mut Vec<InitFragment>,
) {
    let used_name = exports_info.get_used_name(&dep.name).unwrap_or(&dep.name);
    push_init_fragment(
        init_fragments,
        InitFragmentStage::HarmonyExport,
        format!(
            "__webpack_require__.d(__webpack_exports__, {{ {}: () => ({}) }});\n",
            property_name(used_name),
            dep.id
        ),
    );
}

fn apply_harmony_export_expression_dependency(
    dep: &HarmonyExportExpressionDependency,
    exports_info: &ExportsInfo,
    source: &mut ReplaceSource,
    init_fragments: &mut Vec<InitFragment>,
) {
    let binding = dep
        .declaration_id
        .clone()
        .unwrap_or_else(|| "__WEBPACK_DEFAULT_EXPORT__".to_string());
    if dep.declaration_id.is_some() {
        replace(
            source,
            SourceRange::new(dep.statement_range.start, dep.range.start),
            "/* harmony default export */ ".to_string(),
        );
    } else {
        replace(
            source,
            SourceRange::new(dep.statement_range.start, dep.range.start),
            "/* harmony default export */ const __WEBPACK_DEFAULT_EXPORT__ = ".to_string(),
        );
    }
    let used_name = exports_info.get_used_name("default").unwrap_or("default");
    push_init_fragment(
        init_fragments,
        InitFragmentStage::HarmonyExport,
        format!(
            "__webpack_require__.d(__webpack_exports__, {{ {}: () => ({binding}) }});\n",
            property_name(used_name)
        ),
    );
}

fn apply_harmony_export_imported_specifier_dependency(
    dep: &HarmonyExportImportedSpecifierDependency,
    module_id: ModuleId,
    dependency_id: Option<usize>,
    module_graph: &ModuleGraph,
    exports_info: &ExportsInfo,
    module_render_ids: &HashMap<ModuleId, String>,
    init_fragments: &mut Vec<InitFragment>,
) {
    let Some(dependency_id) = dependency_id else {
        return;
    };
    let Some(_target) = module_graph.module_for_dependency(module_id, None, dependency_id) else {
        return;
    };
    let import_var = import_var(&dep.module.request, dep.module.source_order.unwrap_or(0));
    if dep.is_star {
        push_init_fragment(
            init_fragments,
            InitFragmentStage::HarmonyStarReexport,
            format!(
                "/* harmony reexport (unknown) */ for(const __WEBPACK_IMPORT_KEY__ in {import_var}) if(__WEBPACK_IMPORT_KEY__ !== \"default\" && __WEBPACK_IMPORT_KEY__ !== \"__esModule\") __webpack_require__.d(__webpack_exports__, {{ [__WEBPACK_IMPORT_KEY__]: () => ({import_var}[__WEBPACK_IMPORT_KEY__]) }});\n"
            ),
        );
    } else if let Some(name) = &dep.name {
        let expression = export_access_expression(&import_var, &dep.ids);
        let used_name = exports_info.get_used_name(name).unwrap_or(name);
        push_init_fragment(
            init_fragments,
            InitFragmentStage::HarmonyExport,
            format!(
                "__webpack_require__.d(__webpack_exports__, {{ {}: () => ({expression}) }});\n",
                property_name(used_name),
            ),
        );
    }
    let _ = module_render_ids;
}

fn apply_import_dependency(
    dep: &ImportDependency,
    module_id: ModuleId,
    origin_block: Option<usize>,
    dependency_id: Option<usize>,
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    module_render_ids: &HashMap<ModuleId, String>,
    source: &mut ReplaceSource,
) {
    let Some(block_index) = origin_block else {
        return;
    };
    let Some(dependency_id) = dependency_id else {
        return;
    };
    let Some(target) =
        module_graph.module_for_dependency(module_id, Some(block_index), dependency_id)
    else {
        return;
    };
    let target_id = json_string(&module_render_ids[&target]);
    let origin = AsyncBlockOrigin {
        module: module_id,
        block_index,
    };
    let expression = if let Some(group_id) = chunk_graph.block_chunk_group(origin) {
        let group = &chunk_graph.chunk_groups()[group_id.index()];
        let chunk_id = group
            .chunks()
            .first()
            .and_then(|chunk_id| chunk_graph.chunk(*chunk_id))
            .map(|chunk| json_string(chunk.render_id()))
            .unwrap_or_else(|| "\"\"".to_string());
        format!(
            "__webpack_require__.e({chunk_id}).then(__webpack_require__.bind(__webpack_require__, {target_id}))"
        )
    } else {
        format!(
            "Promise.resolve().then(__webpack_require__.bind(__webpack_require__, {target_id}))"
        )
    };
    replace(source, dep.range(), expression);
}

fn replace(source: &mut ReplaceSource, range: SourceRange, content: String) {
    source.replace(range.start, range.end, content, None);
}

#[derive(Debug, Clone)]
struct RenderPathContext {
    raw_context: PathBuf,
    context: PathBuf,
}

impl RenderPathContext {
    fn new(context: &Path) -> Self {
        Self {
            raw_context: context.to_path_buf(),
            context: std::fs::canonicalize(context).unwrap_or_else(|_| context.to_path_buf()),
        }
    }

    fn make_relative(&self, resource: &Path) -> String {
        if let Ok(relative) = resource
            .strip_prefix(&self.context)
            .or_else(|_| resource.strip_prefix(&self.raw_context))
        {
            return normalize_path(relative);
        }

        let resource = std::fs::canonicalize(resource).unwrap_or_else(|_| PathBuf::from(resource));
        let relative = resource.strip_prefix(&self.context).unwrap_or(&resource);
        normalize_path(relative)
    }
}

fn module_render_ids(
    context: &RenderPathContext,
    module_graph: &ModuleGraph,
) -> HashMap<ModuleId, String> {
    module_graph
        .modules()
        .iter()
        .map(|module| (module.id(), module_render_id(context, module)))
        .collect()
}

fn module_render_id(context: &RenderPathContext, module: &Module) -> String {
    let mut resource = context.make_relative(&module.identity().resource);
    if !resource.starts_with("./") {
        resource = format!("./{resource}");
    }
    if let Some(query) = &module.identity().query {
        resource.push('?');
        resource.push_str(query);
    }
    if let Some(fragment) = &module.identity().fragment {
        resource.push('#');
        resource.push_str(fragment);
    }
    resource
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn render_chunk_filename_map(chunk_graph: &ChunkGraph) -> String {
    let mut entries = BTreeMap::new();
    for chunk in chunk_graph.chunks() {
        entries.insert(chunk.render_id().to_string(), chunk.filename().to_string());
    }
    entries
        .into_iter()
        .map(|(chunk_id, filename)| {
            format!("{}: {}", json_string(&chunk_id), json_string(&filename))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn import_var(request: &str, source_order: usize) -> String {
    let ident = sanitize_identifier(request);
    let index = source_order.saturating_sub(1);
    format!("_{ident}__WEBPACK_IMPORTED_MODULE_{index}__")
}

fn import_expression(request: &str, source_order: usize, ids: &[String]) -> String {
    let import_var = import_var(request, source_order);
    export_access_expression(&import_var, ids)
}

fn export_access_expression(base: &str, ids: &[String]) -> String {
    let mut expression = base.to_string();
    for id in ids {
        expression.push_str(&property_access(id));
    }
    expression
}

fn sanitize_identifier(value: &str) -> String {
    let mut ident = value
        .trim_start_matches("./")
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    if ident.is_empty() {
        ident.push_str("module");
    }
    if ident.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        ident.insert(0, '_');
    }
    ident
}

fn property_access(property: &str) -> String {
    if is_identifier(property) {
        format!(".{property}")
    } else {
        format!("[{}]", json_string(property))
    }
}

fn property_name(property: &str) -> String {
    if is_identifier(property) {
        property.to_string()
    } else {
        format!("[{}]", json_string(property))
    }
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first == '$' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

fn json_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeSet, HashMap},
        fs,
    };

    use rspack_sources::{ConcatSource, OriginalSource};

    use crate::{
        CacheOptions, ChunkGraph, ChunkGroupKind, Compiler, CompilerOptions, ConstDependency,
        Dependency, Entry, ModuleGraph, ModuleIdentity, SnapshotOptions, SourceRange,
        build_cache::{BuildCache, CacheIdentifier, CacheKey, CacheNamespace},
    };

    use super::{
        AssetRenderKey, AssetRenderKind, AssetRenderManifest, CodeGenerationInput,
        CodeGenerationKey, CodeGenerationResult, CodeGenerationResults, CodeGenerationSource,
        ModuleRenderManifest, RenderedSource, RuntimeSpec, code_generation_etag, emit_asset,
    };

    #[test]
    fn code_generation_facade_partitions_module_runtime_identity() {
        let build_cache = BuildCache::new(CacheOptions::memory(), SnapshotOptions::default());
        let facade = build_cache.code_generations::<CodeGenerationKey>();
        assert_eq!(
            facade.namespace(),
            CacheNamespace::new("unpack/code-generation")
        );

        let module = ModuleIdentity::new("/project/src/shared.js");
        let main = CodeGenerationKey::new(
            module.clone(),
            RuntimeSpec(BTreeSet::from(["main".to_string()])),
        );
        let same = CodeGenerationKey::new(
            module.clone(),
            RuntimeSpec(BTreeSet::from(["main".to_string()])),
        );
        let admin =
            CodeGenerationKey::new(module, RuntimeSpec(BTreeSet::from(["admin".to_string()])));
        let other_module = CodeGenerationKey::new(
            ModuleIdentity::new("/project/src/other.js"),
            RuntimeSpec(BTreeSet::from(["main".to_string()])),
        );

        assert_eq!(main.cache_identifier(), same.cache_identifier());
        assert_ne!(main.cache_identifier(), admin.cache_identifier());
        assert_ne!(main.cache_identifier(), other_module.cache_identifier());
    }

    #[test]
    fn code_generation_etag_covers_module_hash_and_dependency_template_inputs() {
        let baseline = code_generation_etag_fixture(41, "replacement");
        assert_eq!(baseline, code_generation_etag_fixture(41, "replacement"));
        assert_ne!(baseline, code_generation_etag_fixture(42, "replacement"));
        assert_ne!(baseline, code_generation_etag_fixture(41, "other"));
    }

    fn code_generation_etag_fixture(
        source_hash: u64,
        expression: &str,
    ) -> crate::build_cache::CacheETag {
        let identity = ModuleIdentity::new("/project/src/index.js");
        let mut module_graph = ModuleGraph::default();
        let module_id = module_graph.add_module(identity);
        module_graph
            .module_mut(module_id)
            .expect("fixture module should exist")
            .finish_build(
                Vec::new(),
                Vec::new(),
                vec![Dependency::Const(ConstDependency::new(
                    expression,
                    SourceRange::new(0, 5),
                ))],
                "value".to_string(),
                source_hash,
            );
        let module_render_ids = HashMap::from([(module_id, "./src/index.js".to_string())]);
        code_generation_etag(&CodeGenerationInput {
            module: module_graph
                .module(module_id)
                .expect("fixture module should exist"),
            module_graph: &module_graph,
            chunk_graph: &ChunkGraph::default(),
            module_render_ids: &module_render_ids,
        })
    }

    #[test]
    fn asset_render_facade_uses_stable_namespace_and_manifest_identity() {
        let build_cache = BuildCache::new(CacheOptions::memory(), SnapshotOptions::default());
        let facade = build_cache.asset_renders::<AssetRenderKey>();
        assert_eq!(
            facade.namespace(),
            CacheNamespace::new("unpack/asset-render")
        );

        let initial = AssetRenderKey {
            kind: AssetRenderKind::Initial,
            chunk_id: "main".to_string(),
        };
        let asynchronous = AssetRenderKey {
            kind: AssetRenderKind::Async,
            chunk_id: "main".to_string(),
        };
        assert_eq!(
            initial.cache_identifier(),
            CacheIdentifier::from_parts([b"initial".to_vec(), b"main".to_vec()])
        );
        assert_ne!(initial.cache_identifier(), asynchronous.cache_identifier());
    }

    #[test]
    fn exact_render_hash_covers_generated_source_and_manifest_inputs() {
        let code_generation_key = CodeGenerationKey::new(
            ModuleIdentity::new("/project/src/feature.js"),
            RuntimeSpec(BTreeSet::from(["main".to_string()])),
        );
        let module = ModuleRenderManifest {
            render_id: "./src/feature.js".to_string(),
            code_generation_key: code_generation_key.clone(),
        };
        let render = AssetRenderManifest::AsyncChunk {
            modules: vec![module],
            chunk_id: "src_feature_js".to_string(),
        };
        let mut results = CodeGenerationResults::default();
        results.results.insert(
            code_generation_key.clone(),
            code_generation_result("export const value = 'before';"),
        );
        let before = render.cache_etag(&results);

        results.results.insert(
            code_generation_key,
            code_generation_result("export const value = 'after';"),
        );
        assert_ne!(before, render.cache_etag(&results));

        let initial = AssetRenderManifest::InitialChunk {
            modules: Vec::new(),
            chunk_filename_map: "{\"feature\":\"feature.js\"}".to_string(),
            entry_id: "./src/index.js".to_string(),
            chunk_id: "main".to_string(),
        };
        let changed_filename_map = AssetRenderManifest::InitialChunk {
            modules: Vec::new(),
            chunk_filename_map: "{\"feature\":\"renamed.js\"}".to_string(),
            entry_id: "./src/index.js".to_string(),
            chunk_id: "main".to_string(),
        };
        assert_ne!(
            initial.cache_etag(&results),
            changed_filename_map.cache_etag(&results)
        );
    }

    fn code_generation_result(source: &str) -> CodeGenerationResult {
        CodeGenerationResult::new(CodeGenerationSource::Raw {
            source: source.to_string(),
        })
    }

    #[tokio::test]
    async fn shared_async_chunk_runtime_spec_contains_each_parent_entrypoint()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        fs::write(
            temp.path().join("a.js"),
            "export const load = () => import('./feature');",
        )?;
        fs::write(
            temp.path().join("b.js"),
            "export const load = () => import('./feature');",
        )?;
        fs::write(temp.path().join("feature.js"), "export const value = 42;")?;
        let compilation = Compiler::new(CompilerOptions::new(
            temp.path(),
            vec![Entry::new("a", "./a"), Entry::new("b", "./b")],
        ))
        .run()
        .await?;
        let chunk = compilation
            .chunk_graph()
            .chunks()
            .iter()
            .find(|chunk| {
                chunk.groups().iter().any(|group| {
                    matches!(
                        compilation.chunk_graph().chunk_groups()[group.index()].kind(),
                        ChunkGroupKind::Async
                    )
                })
            })
            .expect("shared async chunk should exist");

        let runtime = RuntimeSpec::for_chunk(compilation.chunk_graph(), chunk);

        assert_eq!(runtime.names().collect::<Vec<_>>(), ["a", "b"]);

        Ok(())
    }

    #[test]
    fn rendered_source_is_reusable_across_asset_filenames() {
        let mut source = ConcatSource::default();
        source.add(OriginalSource::new(
            "export const value = 42;\n",
            "fixture.js",
        ));
        let rendered = RenderedSource::new(source);

        let first = emit_asset("first.js".to_string(), &rendered, true);
        let second = emit_asset("second.js".to_string(), &rendered, true);

        assert_eq!(rendered.source(), "export const value = 42;\n");
        assert_eq!(first[0].filename, "first.js");
        assert!(
            first[0]
                .source
                .ends_with("//# sourceMappingURL=first.js.map\n")
        );
        assert_eq!(second[0].filename, "second.js");
        assert!(
            second[0]
                .source
                .ends_with("//# sourceMappingURL=second.js.map\n")
        );
        assert_eq!(first[1].filename, "first.js.map");
        assert!(first[1].source.contains(r#""file":"first.js""#));
        assert_eq!(second[1].filename, "second.js.map");
        assert!(second[1].source.contains(r#""file":"second.js""#));
    }
}
