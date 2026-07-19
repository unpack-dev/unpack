// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/CodeGenerationResults.js

use std::hash::{Hash, Hasher};

use rspack_sources::{ConcatSource, RawStringSource, SourceMap};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::{
    AsyncBlockOrigin, AsyncDependenciesBlockIndex, Chunk, ChunkGraph, CompilerOptions,
    DependencyIndex, Error, Module, ModuleGraph, ModuleHandle,
    cache::Cache,
    cache_facade::{CacheETag, CacheIdentifier, CacheKey},
    cache_hash::StableHasher,
    code_generation_record::{CodeGenerationRecord, CodeGenerationResult, CodeGenerationSource},
    dependency_template::{json_render_id, json_string},
    id_assignment::RenderId,
    normal_module_factory::{ModuleGeneratorContext, ModuleTypeRegistry},
    rendered_source::RenderedSource,
    runtime::{RuntimeModule, RuntimeRequirements},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub filename: String,
    pub source: String,
    pub binary_source: Option<Vec<u8>>,
}

impl Asset {
    pub fn source_bytes(&self) -> &[u8] {
        self.binary_source
            .as_deref()
            .unwrap_or(self.source.as_bytes())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CodeGenerationResults {
    module_render_ids: FxHashMap<ModuleHandle, RenderId>,
    results: FxHashMap<ModuleHandle, CodeGenerationResult>,
}

pub(crate) struct CodeGenerationOutcome {
    pub(crate) results: CodeGenerationResults,
    pub(crate) errors: Vec<Error>,
}

impl CodeGenerationResults {
    pub(crate) fn runtime_requirements(
        &self,
    ) -> impl Iterator<Item = (ModuleHandle, &RuntimeRequirements)> {
        self.results
            .iter()
            .map(|(module, result)| (*module, result.runtime_requirements()))
    }

    pub(crate) fn module_render_id(&self, module: ModuleHandle) -> Option<&RenderId> {
        self.module_render_ids.get(&module)
    }
}

fn code_generation_etag(input: &ModuleGeneratorContext<'_>) -> CacheETag {
    let ModuleGeneratorContext {
        module,
        module_graph,
        chunk_graph,
        module_render_ids,
    } = input;
    let mut hasher = StableHasher::default();
    hasher.write(b"unpack/code-generation/etag/2");
    // Parsed dependency/template inputs may be changed independently of source
    // text by future module transforms, so they remain part of the item ETag.
    module.source_hash().hash(&mut hasher);
    module
        .build_error()
        .map(ToString::to_string)
        .hash(&mut hasher);
    module.is_harmony().hash(&mut hasher);
    module
        .code_generation_local_input_digest()
        .hash(&mut hasher);
    hash_used_export_names(module, module_graph, &mut hasher);
    module_render_ids.get(&module.handle()).hash(&mut hasher);

    for dependency_index in 0..module.dependencies().len() {
        module_graph
            .module_for_dependency(
                module.handle(),
                None,
                DependencyIndex::new(dependency_index),
            )
            .and_then(|target| module_render_ids.get(&target))
            .hash(&mut hasher);
    }

    for (block_index, block) in module.blocks().iter().enumerate() {
        for dependency_index in 0..block.dependencies().len() {
            module_graph
                .module_for_dependency(
                    module.handle(),
                    Some(AsyncDependenciesBlockIndex::new(block_index)),
                    DependencyIndex::new(dependency_index),
                )
                .and_then(|target| module_render_ids.get(&target))
                .hash(&mut hasher);
        }
        chunk_graph
            .block_chunk_group(AsyncBlockOrigin {
                module: module.handle(),
                block: AsyncDependenciesBlockIndex::new(block_index),
            })
            .and_then(|group_handle| {
                chunk_graph.chunk_groups()[group_handle.index()]
                    .chunks()
                    .first()
            })
            .and_then(|chunk_handle| chunk_graph.chunk(*chunk_handle))
            .map(Chunk::render_id)
            .hash(&mut hasher);
    }

    CacheETag::new(hasher.finish().to_le_bytes())
}

fn hash_used_export_names(module: &Module, module_graph: &ModuleGraph, hasher: &mut StableHasher) {
    for dependency in module
        .presentational_dependencies()
        .iter()
        .chain(module.dependencies())
        .chain(
            module
                .blocks()
                .iter()
                .flat_map(|block| block.dependencies()),
        )
    {
        dependency.update_code_generation_hash(module_graph.exports_info(module.handle()), hasher);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderManifest {
    entries: Vec<RenderManifestEntry>,
}

#[derive(Clone, Copy)]
pub(crate) struct RenderManifestContext<'a> {
    pub module_graph: &'a ModuleGraph,
    pub chunk_graph: &'a ChunkGraph,
    pub entries: &'a [ModuleHandle],
    pub code_generation_results: &'a CodeGenerationResults,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderManifestEntry {
    pub filename: String,
    pub render: RenderManifestContent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RenderManifestContent {
    JavaScript(JavascriptRenderManifest),
    Asset(Asset),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JavascriptRenderManifest {
    InitialChunk {
        modules: Vec<ModuleRenderManifest>,
        runtime_modules: Vec<RenderedRuntimeModule>,
        entry_id: RenderId,
        chunk_id: RenderId,
    },
    AsyncChunk {
        modules: Vec<ModuleRenderManifest>,
        chunk_id: RenderId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModuleRenderManifest {
    pub module: ModuleHandle,
    pub render_id: RenderId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RenderedRuntimeModule {
    pub module: RuntimeModule,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AssetRenderKey {
    kind: AssetRenderKind,
    chunk_id: RenderId,
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
            self.chunk_id.to_string().into_bytes(),
        ])
    }
}

#[cfg(test)]
pub(crate) fn generate_code(
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
) -> CodeGenerationOutcome {
    let module_types = crate::compiler::test_compilation_hooks().normal_module_factory_hooks;
    generate_code_with(module_graph, chunk_graph, |input| {
        generate_registered_module(&module_types, input)
    })
}

pub(crate) fn generate_code_cached(
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    cache: &Cache,
    module_types: &ModuleTypeRegistry,
) -> CodeGenerationOutcome {
    let cache = cache.code_generations();
    generate_code_with(module_graph, chunk_graph, |input| {
        let key = input.module.identity();
        let etag = code_generation_etag(&input);
        if let Some(record) = cache.get(key, Some(&etag)) {
            if record.is_compatible_with(input.module.source()) {
                return Ok(record.as_ref().clone());
            }
        }

        let record = generate_registered_module(module_types, input)?;
        cache.store(key.clone(), Some(etag), record.clone());
        Ok(record)
    })
}

fn generate_registered_module(
    module_types: &ModuleTypeRegistry,
    input: ModuleGeneratorContext<'_>,
) -> Result<CodeGenerationRecord, Error> {
    if let Some(error) = input.module.build_error() {
        return Ok(CodeGenerationRecord::new(CodeGenerationSource::Raw {
            source: render_failed_module_content(error),
        }));
    }
    module_types.generate(input)
}

fn generate_code_with(
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    mut generate_module: impl FnMut(
        ModuleGeneratorContext<'_>,
    ) -> std::result::Result<CodeGenerationRecord, Error>,
) -> CodeGenerationOutcome {
    let module_render_ids = module_graph
        .modules()
        .iter()
        .filter(|module| !chunk_graph.module_chunks(module.handle()).is_empty())
        .map(|module| {
            let render_id = chunk_graph
                .module_render_id(module.handle())
                .unwrap_or_else(|| {
                    panic!(
                        "module {:?} must have an assigned Render ID before code generation",
                        module.handle()
                    )
                })
                .clone();
            (module.handle(), render_id)
        })
        .collect::<FxHashMap<_, _>>();
    let mut results = FxHashMap::default();
    let mut errors = Vec::new();
    for module in module_graph
        .modules()
        .iter()
        .filter(|module| !chunk_graph.module_chunks(module.handle()).is_empty())
    {
        if let Some(concatenated_module) = chunk_graph.concatenated_module(module.handle()) {
            let result = concatenated_module
                .code_generation(module_graph, chunk_graph, &module_render_ids)
                .unwrap_or_else(|error| {
                    errors.push(error.clone());
                    CodeGenerationRecord::new(CodeGenerationSource::Raw {
                        source: render_failed_module_content(&error),
                    })
                    .into_result(module.source())
                    .expect("failed Code Generation source must be compatible")
                });
            let previous = results.insert(module.handle(), result);
            assert!(
                previous.is_none(),
                "module {:?} must be generated exactly once per Compilation",
                module.handle()
            );
            continue;
        }
        let input = ModuleGeneratorContext {
            module,
            module_graph,
            chunk_graph,
            module_render_ids: &module_render_ids,
        };
        let record = generate_module(input).unwrap_or_else(|error| {
            errors.push(error.clone());
            CodeGenerationRecord::new(CodeGenerationSource::Raw {
                source: render_failed_module_content(&error),
            })
        });
        let result = record
            .into_result(module.source())
            .expect("generated Code Generation Record must match its Module source");
        let previous = results.insert(module.handle(), result);
        assert!(
            previous.is_none(),
            "module {:?} must be generated exactly once per Compilation",
            module.handle()
        );
    }

    CodeGenerationOutcome {
        results: CodeGenerationResults {
            module_render_ids,
            results,
        },
        errors,
    }
}

pub(crate) fn create_render_manifest(
    context: RenderManifestContext<'_>,
    render_manifest: &crate::compilation::RenderManifestHook,
) -> RenderManifest {
    let mut manifest_entries = render_manifest.call(context);
    manifest_entries.sort_by(|left, right| left.filename.cmp(&right.filename));
    RenderManifest {
        entries: manifest_entries,
    }
}

pub(crate) fn render_assets(
    options: &CompilerOptions,
    cache: &Cache,
    manifest: &RenderManifest,
    code_generation_results: &CodeGenerationResults,
) -> Vec<Asset> {
    let mut assets = Vec::new();
    let cache = cache.asset_renders::<AssetRenderKey>();
    let cache_enabled = options.cache.kind == crate::CacheKind::Filesystem;
    for entry in &manifest.entries {
        let RenderManifestContent::JavaScript(render) = &entry.render else {
            let RenderManifestContent::Asset(asset) = &entry.render else {
                unreachable!()
            };
            assets.push(asset.clone());
            continue;
        };
        let key = render.cache_key();
        let rendered_source = if cache_enabled {
            let etag = render.cache_etag(code_generation_results);
            if let Some(rendered_source) = cache.get(&key, Some(&etag)) {
                rendered_source.as_ref().clone()
            } else {
                let rendered_source = render_asset(render, code_generation_results);
                cache.store(key, Some(etag), rendered_source.clone());
                rendered_source
            }
        } else {
            render_asset(render, code_generation_results)
        };
        assets.extend(emit_asset(
            entry.filename.clone(),
            &rendered_source,
            options.sourcemap,
        ));
    }
    assets
}

impl JavascriptRenderManifest {
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
        hasher.write(b"unpack/asset-render/hash/2");
        match self {
            Self::InitialChunk {
                modules,
                runtime_modules,
                entry_id,
                chunk_id,
            } => {
                hasher.write_u8(0);
                hash_module_render_inputs(modules, code_generation_results, &mut hasher);
                runtime_modules.hash(&mut hasher);
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
        // Module Handles belong to the current Compilation. They may locate the
        // current result, but must not become Build Cache validation data.
        let result = code_generation_results
            .results
            .get(&module.module)
            .expect("manifest Module must have a Code Generation Result");
        result.source().hash(hasher);
        result
            .runtime_requirements()
            .iter()
            .for_each(|requirement| {
                requirement.hash(hasher);
            });
    }
}

pub(crate) fn module_render_manifest(
    chunk_graph: &ChunkGraph,
    chunk: &Chunk,
    code_generation_results: &CodeGenerationResults,
) -> Vec<ModuleRenderManifest> {
    let mut modules = chunk_graph
        .chunk_modules(chunk.handle())
        .iter()
        .map(|module_handle| {
            assert!(
                code_generation_results.results.contains_key(module_handle),
                "module {module_handle:?} must have a Code Generation Result before rendering"
            );
            ModuleRenderManifest {
                module: *module_handle,
                render_id: code_generation_results
                    .module_render_ids
                    .get(module_handle)
                    .unwrap_or_else(|| {
                        panic!("module {module_handle:?} must have a Render ID before rendering")
                    })
                    .clone(),
            }
        })
        .collect::<Vec<_>>();
    modules.sort_by(|left, right| left.render_id.cmp(&right.render_id));
    modules
}

fn render_asset(
    manifest: &JavascriptRenderManifest,
    code_generation_results: &CodeGenerationResults,
) -> RenderedSource {
    let source = match manifest {
        JavascriptRenderManifest::InitialChunk {
            modules,
            runtime_modules,
            entry_id,
            chunk_id: _,
        } => render_initial_asset(modules, runtime_modules, entry_id, code_generation_results),
        JavascriptRenderManifest::AsyncChunk { modules, chunk_id } => {
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

    assets.push(Asset {
        filename,
        source,
        binary_source: None,
    });

    if let Some(map) = source_map {
        assets.push(Asset {
            filename: map_filename,
            source: map.to_json(),
            binary_source: None,
        });
    }

    assets
}

fn render_initial_asset(
    modules: &[ModuleRenderManifest],
    runtime_modules: &[RenderedRuntimeModule],
    entry_id: &RenderId,
    code_generation_results: &CodeGenerationResults,
) -> ConcatSource {
    let modules = render_module_table(modules, code_generation_results);
    let entry_id = json_render_id(entry_id);

    let mut source = ConcatSource::default();
    source.add(RawStringSource::from(
        r#""use strict";
var __webpack_modules__ = ({
"#
        .to_string(),
    ));
    source.add(modules);
    source.add(RawStringSource::from(
        r#"
});

var __webpack_module_cache__ = {};
function __webpack_require__(moduleId) {
  var cachedModule = __webpack_module_cache__[moduleId];
  if (cachedModule !== undefined) {
    return cachedModule.exports;
  }
  var module = __webpack_module_cache__[moduleId] = {
    exports: {}
  };
  __webpack_modules__[moduleId](module, module.exports, __webpack_require__);
  return module.exports;
}
"#
        .to_string(),
    ));
    for runtime_module in runtime_modules {
        source.add(RawStringSource::from(runtime_module.source.clone()));
    }
    source.add(RawStringSource::from(format!(
        "module.exports = __webpack_require__({entry_id});\n"
    )));
    source
}

fn render_async_chunk_asset(
    modules: &[ModuleRenderManifest],
    chunk_id: &RenderId,
    code_generation_results: &CodeGenerationResults,
) -> ConcatSource {
    let modules = render_module_table(modules, code_generation_results);
    let chunk_id = json_render_id(chunk_id);
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
        let result = code_generation_results
            .results
            .get(&module.module)
            .unwrap_or_else(|| {
                panic!(
                    "module {:?} must have a Code Generation Result before rendering",
                    module.module
                )
            });
        if first {
            first = false;
        } else {
            source.add(RawStringSource::from(",\n".to_string()));
        }
        source.add(RawStringSource::from(format!(
            "{}: ((__unused_webpack_module, __webpack_exports__, __webpack_require__) => {{\n\"use strict\";\n",
            json_render_id(&module.render_id),
        )));
        source.add(result.source().clone());
        source.add(RawStringSource::from("\n})".to_string()));
    }
    source
}

fn render_failed_module_content(error: &Error) -> String {
    format!("throw new Error({});", json_string(&error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rspack_sources::{ConcatSource, OriginalSource, ReplacementEnforce, Source};
    use rustc_hash::FxHashMap;

    use crate::{
        CacheOptions, Compiler, CompilerOptions, ConstDependency, Dependency, Entry, Error,
        ModuleHandle, ModuleIdentity, SnapshotOptions, SourceRange,
        cache::{Cache, CacheItemFamily, CacheItemWork},
        cache_facade::{CacheIdentifier, CacheKey, CacheNamespace},
        id_assignment::{RenderId, assign_chunk_render_ids, assign_module_render_ids},
        runtime::{RuntimeModule, RuntimeRequirement},
    };

    use super::{
        AssetRenderKey, AssetRenderKind, CodeGenerationResult, CodeGenerationResults,
        CodeGenerationSource, JavascriptRenderManifest, ModuleRenderManifest, RenderManifest,
        RenderManifestContent, RenderManifestEntry, RenderedRuntimeModule, RenderedSource,
        emit_asset, render_assets,
    };
    use crate::code_generation_record::{CodeGenerationRecord, CodeGenerationReplacement};

    #[test]
    fn asset_render_facade_uses_stable_namespace_and_manifest_identity() {
        let cache = Cache::new(CacheOptions::memory(), SnapshotOptions::default());
        let facade = cache.asset_renders::<AssetRenderKey>();
        assert_eq!(
            facade.namespace(),
            CacheNamespace::new("unpack/asset-render")
        );

        let initial = AssetRenderKey {
            kind: AssetRenderKind::Initial,
            chunk_id: RenderId::String("main".to_string()),
        };
        let asynchronous = AssetRenderKey {
            kind: AssetRenderKind::Async,
            chunk_id: RenderId::String("main".to_string()),
        };
        assert_eq!(
            initial.cache_identifier(),
            CacheIdentifier::from_parts([b"initial".to_vec(), b"main".to_vec()])
        );
        assert_ne!(initial.cache_identifier(), asynchronous.cache_identifier());
    }

    #[test]
    fn asset_render_cache_reuses_equivalent_inputs_with_different_graph_handles() {
        let temp = tempfile::tempdir().expect("create asset render cache directory");
        let mut options = CompilerOptions::new("/project", Vec::new());
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(temp.path().join("cache"));
        options.sourcemap = false;
        let cache = Cache::new(options.cache.clone(), SnapshotOptions::default());
        let render_id = RenderId::String("./src/feature.js".to_string());
        let chunk_id = RenderId::String("src_feature_js".to_string());

        let render_with_handle = |module_handle| {
            let manifest = RenderManifest {
                entries: vec![RenderManifestEntry {
                    filename: "src_feature_js.js".to_string(),
                    render: RenderManifestContent::JavaScript(
                        JavascriptRenderManifest::AsyncChunk {
                            modules: vec![ModuleRenderManifest {
                                module: module_handle,
                                render_id: render_id.clone(),
                            }],
                            chunk_id: chunk_id.clone(),
                        },
                    ),
                }],
            };
            let mut results = CodeGenerationResults::default();
            results.results.insert(
                module_handle,
                code_generation_result("export const value = 'stable';"),
            );
            render_assets(&options, &cache, &manifest, &results)
        };

        let first = render_with_handle(ModuleHandle::new(0));
        let second = render_with_handle(ModuleHandle::new(1));

        assert_eq!(first, second);
        assert_eq!(
            cache
                .work_counters()
                .for_family(CacheItemFamily::AssetRender),
            CacheItemWork {
                hits: 1,
                misses: 1,
                stores: 1,
                restores: 0,
                evictions: 0,
            }
        );
    }

    #[test]
    fn exact_render_hash_covers_generated_source_and_manifest_inputs() {
        let module_handle = ModuleHandle::new(0);
        let module = ModuleRenderManifest {
            module: module_handle,
            render_id: RenderId::String("./src/feature.js".to_string()),
        };
        let render = JavascriptRenderManifest::AsyncChunk {
            modules: vec![module],
            chunk_id: RenderId::String("src_feature_js".to_string()),
        };
        let mut results = CodeGenerationResults::default();
        results.results.insert(
            module_handle,
            code_generation_result("export const value = 'before';"),
        );
        let before = render.cache_etag(&results);

        results.results.insert(
            module_handle,
            code_generation_result("export const value = 'after';"),
        );
        assert_ne!(before, render.cache_etag(&results));

        let without_requirement = render.cache_etag(&results);
        let mut requirements = super::RuntimeRequirements::default();
        requirements.insert(RuntimeRequirement::MakeNamespaceObject);
        results.results.insert(
            module_handle,
            code_generation_result("export const value = 'after';")
                .with_runtime_requirements(requirements),
        );
        assert_ne!(without_requirement, render.cache_etag(&results));

        let initial = JavascriptRenderManifest::InitialChunk {
            modules: Vec::new(),
            runtime_modules: vec![RenderedRuntimeModule {
                module: RuntimeModule::GetChunkFilename,
                source: "return feature.js".to_string(),
            }],
            entry_id: RenderId::String("./src/index.js".to_string()),
            chunk_id: RenderId::String("main".to_string()),
        };
        let changed_filename_map = JavascriptRenderManifest::InitialChunk {
            modules: Vec::new(),
            runtime_modules: vec![RenderedRuntimeModule {
                module: RuntimeModule::GetChunkFilename,
                source: "return renamed.js".to_string(),
            }],
            entry_id: RenderId::String("./src/index.js".to_string()),
            chunk_id: RenderId::String("main".to_string()),
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

    #[test]
    fn code_generation_cache_invalidates_template_inputs_when_source_is_unchanged() {
        let options = CompilerOptions::new("/project", vec![Entry::new("main", "./index")]);
        let build = |expression: &str| {
            let mut building_module_graph = crate::module_graph::BuildingModuleGraph::default();
            let module =
                building_module_graph.add_module(ModuleIdentity::new("/project/index.js"), None);
            building_module_graph
                .finish_module_build(
                    module,
                    crate::module::BuiltModuleContent::from_test_parts(
                        Vec::new(),
                        Vec::new(),
                        vec![Dependency::Const(ConstDependency::new(
                            expression,
                            SourceRange::new(0, 5),
                        ))],
                        "value".to_string(),
                        1,
                    ),
                )
                .expect("fixture Module should exist");
            let module_graph = building_module_graph.finish();
            let mut chunk_graph =
                crate::build_chunk_graph::build_chunk_graph(&options, &module_graph, &[module]);
            assign_module_render_ids(&options, &module_graph, &mut chunk_graph);
            assign_chunk_render_ids(&options, &module_graph, &mut chunk_graph);
            (module_graph, chunk_graph, module)
        };
        let cache = Cache::new(CacheOptions::memory(), SnapshotOptions::default());

        let (first_graph, first_chunks, first_module) = build("first");
        let module_types = crate::compiler::test_compilation_hooks().normal_module_factory_hooks;
        let first = super::generate_code_cached(&first_graph, &first_chunks, &cache, &module_types);
        assert_eq!(
            first.results.results[&first_module]
                .source()
                .source()
                .into_string_lossy(),
            "first"
        );

        let (second_graph, second_chunks, second_module) = build("second");
        let second =
            super::generate_code_cached(&second_graph, &second_chunks, &cache, &module_types);
        assert_eq!(
            second.results.results[&second_module]
                .source()
                .source()
                .into_string_lossy(),
            "second"
        );
        assert_eq!(
            cache
                .work_counters()
                .for_family(CacheItemFamily::CodeGeneration),
            CacheItemWork {
                hits: 0,
                misses: 2,
                stores: 2,
                restores: 0,
                evictions: 0,
            }
        );
    }

    #[test]
    fn incompatible_cached_replacement_recipe_is_regenerated() {
        let options = CompilerOptions::new("/project", vec![Entry::new("main", "./index")]);
        let mut building_module_graph = crate::module_graph::BuildingModuleGraph::default();
        let module =
            building_module_graph.add_module(ModuleIdentity::new("/project/index.js"), None);
        building_module_graph
            .finish_module_build(
                module,
                crate::module::BuiltModuleContent::from_test_parts(
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    "éx".to_string(),
                    1,
                ),
            )
            .expect("fixture Module should exist");
        let module_graph = building_module_graph.finish();
        let mut chunk_graph =
            crate::build_chunk_graph::build_chunk_graph(&options, &module_graph, &[module]);
        assign_module_render_ids(&options, &module_graph, &mut chunk_graph);
        assign_chunk_render_ids(&options, &module_graph, &mut chunk_graph);
        let module_render_ids = FxHashMap::from_iter([(
            module,
            chunk_graph
                .module_render_id(module)
                .expect("fixture Module should have a Render ID")
                .clone(),
        )]);
        let module_ref = module_graph
            .module(module)
            .expect("fixture Module should exist");
        let input = crate::normal_module_factory::ModuleGeneratorContext {
            module: module_ref,
            module_graph: &module_graph,
            chunk_graph: &chunk_graph,
            module_render_ids: &module_render_ids,
        };
        let etag = super::code_generation_etag(&input);
        let cache = Cache::new(CacheOptions::memory(), SnapshotOptions::default());
        let code_generation_cache = cache.code_generations();
        code_generation_cache.store(
            module_ref.identity().clone(),
            Some(etag.clone()),
            CodeGenerationRecord::new(CodeGenerationSource::OriginalWithReplacements {
                prefix: String::new(),
                original_source_len: 3,
                original_name: "fixture.js".to_string(),
                replacements: vec![CodeGenerationReplacement {
                    start: 1,
                    end: 1,
                    content: "invalid-boundary".to_string(),
                    name: None,
                    enforce: ReplacementEnforce::Normal,
                }],
                suffix: String::new(),
            }),
        );

        let outcome = super::generate_code_cached(
            &module_graph,
            &chunk_graph,
            &cache,
            &crate::compiler::test_compilation_hooks().normal_module_factory_hooks,
        );
        assert_eq!(
            outcome.results.results[&module]
                .source()
                .source()
                .into_string_lossy(),
            "éx"
        );
        assert!(
            code_generation_cache
                .get(module_ref.identity(), Some(&etag))
                .expect("regenerated Code Generation Record should be stored")
                .is_compatible_with(module_ref.source())
        );
    }

    #[test]
    fn module_attributable_generation_errors_become_throwing_results() {
        let options = CompilerOptions::new("/project", vec![Entry::new("main", "./index")]);
        let mut building_module_graph = crate::module_graph::BuildingModuleGraph::default();
        let module =
            building_module_graph.add_module(ModuleIdentity::new("/project/index.js"), None);
        building_module_graph
            .finish_module_build(
                module,
                crate::module::BuiltModuleContent::from_test_parts(
                    Vec::new(),
                    Vec::new(),
                    vec![Dependency::Const(ConstDependency::new(
                        "replacement",
                        SourceRange::new(0, 99),
                    ))],
                    "value".to_string(),
                    1,
                ),
            )
            .expect("fixture Module should exist");
        let module_graph = building_module_graph.finish();
        let mut chunk_graph =
            crate::build_chunk_graph::build_chunk_graph(&options, &module_graph, &[module]);
        assign_module_render_ids(&options, &module_graph, &mut chunk_graph);
        assign_chunk_render_ids(&options, &module_graph, &mut chunk_graph);

        let cache = Cache::new(CacheOptions::memory(), SnapshotOptions::default());
        for outcome in [
            super::generate_code_cached(
                &module_graph,
                &chunk_graph,
                &cache,
                &crate::compiler::test_compilation_hooks().normal_module_factory_hooks,
            ),
            super::generate_code_cached(
                &module_graph,
                &chunk_graph,
                &cache,
                &crate::compiler::test_compilation_hooks().normal_module_factory_hooks,
            ),
        ] {
            assert_eq!(
                outcome.errors,
                [Error::CodeGeneration {
                    module,
                    path: "/project/index.js".into(),
                    message: "dependency source range 0..99 exceeds module source length 5"
                        .to_string(),
                }]
            );
            assert!(outcome.errors[0].is_compilation_error());
            assert!(
                outcome.results.results[&module]
                    .source()
                    .source()
                    .into_string_lossy()
                    .contains("throw new Error")
            );
        }
        assert_eq!(
            cache
                .work_counters()
                .for_family(CacheItemFamily::CodeGeneration),
            CacheItemWork {
                hits: 0,
                misses: 2,
                stores: 0,
                restores: 0,
                evictions: 0,
            }
        );
    }

    #[tokio::test]
    async fn code_generation_is_module_only_and_leaves_factory_wrapping_to_renderer()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        fs::write(
            temp.path().join("a.js"),
            "import { shared } from './shared'; export const a = shared;",
        )?;
        fs::write(
            temp.path().join("b.js"),
            "import { shared } from './shared'; export const b = shared;",
        )?;
        fs::write(temp.path().join("shared.js"), "export const shared = 42;")?;
        let compilation = Compiler::new(CompilerOptions::new(
            temp.path(),
            vec![Entry::new("a", "./a"), Entry::new("b", "./b")],
        ))
        .run()
        .await?;
        for module in compilation.module_graph().modules() {
            let requirements = compilation
                .chunk_graph()
                .module_runtime_requirements(module.handle())
                .expect("renderable Module must have processed Runtime Requirements");
            assert!(requirements.contains(RuntimeRequirement::MakeNamespaceObject));
            assert!(requirements.contains(RuntimeRequirement::DefinePropertyGetters));
            assert!(requirements.contains(RuntimeRequirement::HasOwnProperty));
        }
        for chunk in compilation.chunk_graph().chunks() {
            let requirements = compilation
                .chunk_graph()
                .chunk_runtime_requirements(chunk.handle())
                .expect("Chunk must have processed Runtime Requirements");
            assert!(requirements.contains(RuntimeRequirement::MakeNamespaceObject));
            assert!(requirements.contains(RuntimeRequirement::DefinePropertyGetters));
            assert!(requirements.contains(RuntimeRequirement::HasOwnProperty));
            assert_eq!(
                compilation.chunk_graph().runtime_modules(chunk.handle()),
                [
                    RuntimeModule::DefinePropertyGetters,
                    RuntimeModule::HasOwnProperty,
                    RuntimeModule::MakeNamespaceObject,
                ]
            );
        }
        for entrypoint in compilation.chunk_graph().entrypoints() {
            let requirements = compilation
                .chunk_graph()
                .runtime_tree_requirements(*entrypoint)
                .expect("Entrypoint must have processed runtime-tree requirements");
            assert!(requirements.contains(RuntimeRequirement::ModuleFactories));
            assert!(requirements.contains(RuntimeRequirement::ModuleCache));
            assert!(requirements.contains(RuntimeRequirement::Require));
            assert!(requirements.contains(RuntimeRequirement::ReturnExportsFromRuntime));
        }
        let mut generation_count = 0;
        let outcome = super::generate_code_with(
            compilation.module_graph(),
            compilation.chunk_graph(),
            |input| {
                generation_count += 1;
                crate::javascript::javascript_generator::generate(input)
            },
        );
        assert!(outcome.errors.is_empty());
        let results = outcome.results;

        assert_eq!(generation_count, compilation.module_graph().modules().len());
        assert_eq!(
            results.results.len(),
            compilation.module_graph().modules().len()
        );
        assert!(results.results.values().all(|result| {
            !result
                .source()
                .source()
                .into_string_lossy()
                .contains("__unused_webpack_module")
        }));
        assert!(results.results.values().all(|result| {
            result
                .runtime_requirements()
                .contains(RuntimeRequirement::MakeNamespaceObject)
        }));
        assert!(results.results.values().all(|result| {
            result
                .runtime_requirements()
                .contains(RuntimeRequirement::DefinePropertyGetters)
        }));
        assert!(results.results.values().any(|result| {
            result
                .runtime_requirements()
                .contains(RuntimeRequirement::Require)
        }));

        let failed_module = compilation
            .module_graph()
            .modules()
            .iter()
            .find(|module| module.identity().resource.ends_with("shared.js"))
            .expect("fixture shared Module should exist");
        let failed_path = failed_module.identity().resource.clone();
        let failed_module = failed_module.handle();
        let failed = super::generate_code_with(
            compilation.module_graph(),
            compilation.chunk_graph(),
            |input| {
                if input.module.handle() == failed_module {
                    Err(Error::CodeGeneration {
                        module: failed_module,
                        path: input.module.identity().resource.clone(),
                        message: "fixture generation failure".to_string(),
                    })
                } else {
                    crate::javascript::javascript_generator::generate(input)
                }
            },
        );
        assert_eq!(
            failed.errors,
            [Error::CodeGeneration {
                module: failed_module,
                path: failed_path,
                message: "fixture generation failure".to_string(),
            }]
        );
        assert!(
            failed.results.results[&failed_module]
                .source()
                .source()
                .into_string_lossy()
                .contains("fixture generation failure")
        );

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
