// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/CodeGenerationResults.js

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

use rspack_sources::{ConcatSource, OriginalSource, RawStringSource, ReplaceSource, SourceMap};
use serde::{Deserialize, Serialize};

use crate::{
    AsyncBlockOrigin, AsyncDependenciesBlockIndex, Chunk, ChunkGraph, ChunkGroupKind,
    CompilerOptions, ConstDependency, Dependency, DependencyIndex, Error, ExportsInfo,
    HarmonyExportExpressionDependency, HarmonyExportHeaderDependency,
    HarmonyExportImportedSpecifierDependency, HarmonyExportSpecifierDependency,
    HarmonyImportSideEffectDependency, HarmonyImportSpecifierDependency, ImportDependency, Module,
    ModuleGraph, ModuleHandle, ModuleType, SourceRange,
    cache::BuildCache,
    cache_facade::{CacheETag, CacheIdentifier, CacheKey},
    cache_hash::StableHasher,
    code_generation_record::{
        CodeGenerationRecord, CodeGenerationReplacement, CodeGenerationResult, CodeGenerationSource,
    },
    id_assignment::RenderId,
    output_filename::resolve_chunk_filename,
    rendered_source::RenderedSource,
    runtime::{RuntimeModule, RuntimeModuleContext, RuntimeRequirement, RuntimeRequirements},
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
    module_render_ids: HashMap<ModuleHandle, RenderId>,
    results: HashMap<ModuleHandle, CodeGenerationResult>,
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
}

struct CodeGenerationInput<'a> {
    module: &'a Module,
    module_graph: &'a ModuleGraph,
    chunk_graph: &'a ChunkGraph,
    module_render_ids: &'a HashMap<ModuleHandle, RenderId>,
}

fn code_generation_etag(input: &CodeGenerationInput<'_>) -> CacheETag {
    let CodeGenerationInput {
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
    hash_used_export_names(module, &mut hasher);
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

fn hash_used_export_names(module: &Module, hasher: &mut StableHasher) {
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
        match dependency {
            Dependency::HarmonyExportSpecifier(dependency) => {
                hasher.write_u8(0);
                module
                    .exports_info()
                    .get_used_name(&dependency.name)
                    .hash(hasher);
            }
            Dependency::HarmonyExportExpression(_) => {
                hasher.write_u8(1);
                module.exports_info().get_used_name("default").hash(hasher);
            }
            Dependency::HarmonyExportImportedSpecifier(dependency) => {
                hasher.write_u8(2);
                dependency
                    .name
                    .as_deref()
                    .and_then(|name| module.exports_info().get_used_name(name))
                    .hash(hasher);
            }
            _ => {}
        }
    }
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
struct ModuleRenderManifest {
    module: ModuleHandle,
    render_id: RenderId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RenderedRuntimeModule {
    module: RuntimeModule,
    source: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct InitFragment {
    stage: InitFragmentStage,
    order: usize,
    content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum InitFragmentStage {
    HarmonyCompatibility,
    HarmonyExport,
    HarmonyImport,
    HarmonyStarReexport,
}

#[cfg(test)]
pub(crate) fn generate_code(
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
) -> CodeGenerationOutcome {
    generate_code_with(module_graph, chunk_graph, generate_module_code)
}

pub(crate) fn generate_code_cached(
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    build_cache: &BuildCache,
) -> CodeGenerationOutcome {
    let cache = build_cache.code_generations();
    generate_code_with(module_graph, chunk_graph, |input| {
        let key = input.module.identity().clone();
        let etag = code_generation_etag(&input);
        if let Some(record) = cache.get(&key, Some(&etag)) {
            if record.is_compatible_with(input.module.source()) {
                return Ok(record.as_ref().clone());
            }
        }

        let record = generate_module_code(input)?;
        cache.store(key, Some(etag), record.clone());
        Ok(record)
    })
}

fn generate_code_with(
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    mut generate_module: impl FnMut(
        CodeGenerationInput<'_>,
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
        .collect::<HashMap<_, _>>();
    let mut results = HashMap::new();
    let mut errors = Vec::new();
    for module in module_graph
        .modules()
        .iter()
        .filter(|module| !chunk_graph.module_chunks(module.handle()).is_empty())
    {
        let input = CodeGenerationInput {
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
    chunk_graph: &ChunkGraph,
    entries: &[ModuleHandle],
    code_generation_results: &CodeGenerationResults,
) -> RenderManifest {
    let mut manifest_entries = Vec::new();

    for (entry_index, group_handle) in chunk_graph.entrypoints().iter().copied().enumerate() {
        let group = &chunk_graph.chunk_groups()[group_handle.index()];
        let chunk_handle = group
            .chunks()
            .first()
            .copied()
            .expect("Entrypoint must contain a Chunk before manifest creation");
        let chunk = chunk_graph
            .chunk(chunk_handle)
            .expect("Entrypoint Chunk must exist before manifest creation");
        let entry_module = entries
            .get(entry_index)
            .copied()
            .expect("Entrypoint must have an Entry Module before manifest creation");
        let modules = module_render_manifest(chunk_graph, chunk, code_generation_results);
        chunk_graph
            .runtime_tree_requirements(group_handle)
            .expect("Runtime Requirements must be processed before manifest creation");
        let runtime_context = RuntimeModuleContext {
            chunk_graph,
            runtime_chunk: chunk_handle,
        };
        let runtime_modules = chunk_graph
            .runtime_modules(chunk_handle)
            .iter()
            .map(|module| RenderedRuntimeModule {
                module: *module,
                source: module.generate(&runtime_context),
            })
            .collect();
        manifest_entries.push(RenderManifestEntry {
            filename: resolve_chunk_filename(chunk),
            render: AssetRenderManifest::InitialChunk {
                modules,
                runtime_modules,
                entry_id: code_generation_results
                    .module_render_ids
                    .get(&entry_module)
                    .expect("Entry Module must have a Render ID before manifest creation")
                    .clone(),
                chunk_id: chunk.render_id().clone(),
            },
        });
    }

    for chunk in chunk_graph.chunks() {
        let is_initial = chunk.groups().iter().any(|group_handle| {
            matches!(
                chunk_graph.chunk_groups()[group_handle.index()].kind(),
                ChunkGroupKind::Entrypoint { .. }
            )
        });
        if is_initial {
            continue;
        }
        manifest_entries.push(RenderManifestEntry {
            filename: resolve_chunk_filename(chunk),
            render: AssetRenderManifest::AsyncChunk {
                modules: module_render_manifest(chunk_graph, chunk, code_generation_results),
                chunk_id: chunk.render_id().clone(),
            },
        });
    }

    manifest_entries.sort_by(|left, right| left.filename.cmp(&right.filename));
    RenderManifest {
        entries: manifest_entries,
    }
}

pub(crate) fn render_assets(
    options: &CompilerOptions,
    build_cache: &BuildCache,
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    manifest: &RenderManifest,
    code_generation_results: &CodeGenerationResults,
) -> Vec<Asset> {
    let mut assets = Vec::new();
    let cache = build_cache.asset_renders::<AssetRenderKey>();
    let cache_enabled = options.cache.kind == crate::CacheKind::Filesystem;
    for entry in &manifest.entries {
        let key = entry.render.cache_key();
        let rendered_source = if cache_enabled {
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
    assets.extend(crate::asset_generator::render_resource_assets(
        module_graph,
        chunk_graph,
    ));
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
        module.module.hash(hasher);
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

fn module_render_manifest(
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
    manifest: &AssetRenderManifest,
    code_generation_results: &CodeGenerationResults,
) -> RenderedSource {
    let source = match manifest {
        AssetRenderManifest::InitialChunk {
            modules,
            runtime_modules,
            entry_id,
            chunk_id: _,
        } => render_initial_asset(modules, runtime_modules, entry_id, code_generation_results),
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

fn generate_module_code(
    input: CodeGenerationInput<'_>,
) -> std::result::Result<CodeGenerationRecord, Error> {
    let CodeGenerationInput {
        module,
        module_graph,
        chunk_graph,
        module_render_ids,
    } = input;
    if let Some(error) = module.build_error() {
        return Ok(CodeGenerationRecord::new(CodeGenerationSource::Raw {
            source: render_failed_module_content(error),
        }));
    }

    if module.identity().module_type == ModuleType::Json {
        return Ok(crate::json_generator::generate(module.source()));
    }
    if module.identity().module_type.is_asset() {
        return Ok(crate::asset_generator::generate(module));
    }

    let module_handle = module.handle();
    let module_render_id = &module_render_ids[&module_handle];
    let module_render_name = module_render_id.to_string();
    let mut source = ReplaceSource::new(OriginalSource::new(
        module.source(),
        module_render_name.as_str(),
    ));
    let mut init_fragments = Vec::new();
    let mut runtime_requirements = RuntimeRequirements::default();
    if module.is_harmony() {
        apply_harmony_compatibility_template(&mut runtime_requirements, &mut init_fragments);
    }

    for dependency in module.presentational_dependencies() {
        apply_dependency_template(
            dependency,
            module_handle,
            None,
            None,
            module_graph,
            chunk_graph,
            module.exports_info(),
            module_render_ids,
            &mut runtime_requirements,
            &mut source,
            &mut init_fragments,
        )?;
    }
    for (dependency_index, dependency) in module.dependencies().iter().enumerate() {
        apply_dependency_template(
            dependency,
            module_handle,
            None,
            Some(DependencyIndex::new(dependency_index)),
            module_graph,
            chunk_graph,
            module.exports_info(),
            module_render_ids,
            &mut runtime_requirements,
            &mut source,
            &mut init_fragments,
        )?;
    }
    for (block_index, block) in module.blocks().iter().enumerate() {
        for (dependency_index, dependency) in block.dependencies().iter().enumerate() {
            apply_dependency_template(
                dependency,
                module_handle,
                Some(AsyncDependenciesBlockIndex::new(block_index)),
                Some(DependencyIndex::new(dependency_index)),
                module_graph,
                chunk_graph,
                module.exports_info(),
                module_render_ids,
                &mut runtime_requirements,
                &mut source,
                &mut init_fragments,
            )?;
        }
    }

    let init = render_init_fragments(init_fragments);
    Ok(
        CodeGenerationRecord::new(CodeGenerationSource::OriginalWithReplacements {
            prefix: init,
            original_source_len: u32::try_from(module.source_len())
                .expect("Module source length must fit the Code Generation cache format"),
            original_name: module_render_name,
            replacements: source
                .replacements()
                .iter()
                .map(CodeGenerationReplacement::from)
                .collect(),
            suffix: String::new(),
        })
        .with_runtime_requirements(runtime_requirements),
    )
}

fn render_failed_module_content(error: &Error) -> String {
    format!("throw new Error({});", json_string(&error.to_string()))
}

fn apply_harmony_compatibility_template(
    runtime_requirements: &mut RuntimeRequirements,
    init_fragments: &mut Vec<InitFragment>,
) {
    runtime_requirements.insert(RuntimeRequirement::MakeNamespaceObject);
    push_init_fragment(
        init_fragments,
        InitFragmentStage::HarmonyCompatibility,
        "__webpack_require__.r(__webpack_exports__);\n".to_string(),
    );
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
    module_handle: ModuleHandle,
    origin_block: Option<AsyncDependenciesBlockIndex>,
    dependency_index: Option<DependencyIndex>,
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    exports_info: &ExportsInfo,
    module_render_ids: &HashMap<ModuleHandle, RenderId>,
    runtime_requirements: &mut RuntimeRequirements,
    source: &mut ReplaceSource,
    init_fragments: &mut Vec<InitFragment>,
) -> std::result::Result<(), Error> {
    let module = module_graph
        .module(module_handle)
        .expect("Dependency Template origin Module must exist in the Module Graph");
    for range in dependency.source_ranges() {
        if range.start > range.end || range.end as usize > module.source_len() {
            return Err(Error::CodeGeneration {
                module: module_handle,
                path: module.identity().resource.clone(),
                message: format!(
                    "dependency source range {}..{} exceeds module source length {}",
                    range.start,
                    range.end,
                    module.source_len()
                ),
            });
        }
    }
    match dependency {
        Dependency::Const(dep) => apply_const_dependency(dep, source),
        Dependency::Null(_) => {}
        Dependency::HarmonyExportHeader(dep) => apply_export_header_dependency(dep, source),
        Dependency::HarmonyImportSideEffect(dep) => {
            runtime_requirements.insert(RuntimeRequirement::Require);
            apply_harmony_import_side_effect_dependency(
                dep,
                module_handle,
                dependency_index,
                module_graph,
                module_render_ids,
                init_fragments,
            )
        }
        Dependency::HarmonyImportSpecifier(dep) => apply_harmony_import_specifier_dependency(
            dep,
            module_handle,
            dependency_index,
            module_graph,
            module_render_ids,
            source,
        ),
        Dependency::HarmonyExportSpecifier(dep) => {
            runtime_requirements.insert(RuntimeRequirement::DefinePropertyGetters);
            apply_harmony_export_specifier_dependency(dep, exports_info, init_fragments)
        }
        Dependency::HarmonyExportExpression(dep) => {
            runtime_requirements.insert(RuntimeRequirement::DefinePropertyGetters);
            apply_harmony_export_expression_dependency(dep, exports_info, source, init_fragments)
        }
        Dependency::HarmonyExportImportedSpecifier(dep) => {
            runtime_requirements.insert(RuntimeRequirement::DefinePropertyGetters);
            apply_harmony_export_imported_specifier_dependency(
                dep,
                module_handle,
                dependency_index,
                module_graph,
                exports_info,
                module_render_ids,
                init_fragments,
            )
        }
        Dependency::Import(dep) => {
            runtime_requirements.insert(RuntimeRequirement::Require);
            apply_import_dependency(
                dep,
                module_handle,
                origin_block,
                dependency_index,
                module_graph,
                chunk_graph,
                module_render_ids,
                runtime_requirements,
                source,
            )
        }
        Dependency::Entry(_) => {}
    }
    Ok(())
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
    module_handle: ModuleHandle,
    dependency_index: Option<DependencyIndex>,
    module_graph: &ModuleGraph,
    module_render_ids: &HashMap<ModuleHandle, RenderId>,
    init_fragments: &mut Vec<InitFragment>,
) {
    let dependency_index = dependency_index.expect("Harmony import must have a Dependency Index");
    let target = module_graph
        .module_for_dependency(module_handle, None, dependency_index)
        .expect("Harmony import must have a Module Graph connection");
    let Some(target_render_id) = module_render_ids.get(&target) else {
        return;
    };
    let import_var = import_var(&dep.module.request, dep.module.source_order.unwrap_or(0));
    let target_id = json_render_id(target_render_id);
    push_init_fragment(
        init_fragments,
        InitFragmentStage::HarmonyImport,
        format!("/* harmony import */ var {import_var} = __webpack_require__({target_id});\n"),
    );
}

fn apply_harmony_import_specifier_dependency(
    dep: &HarmonyImportSpecifierDependency,
    module_handle: ModuleHandle,
    dependency_index: Option<DependencyIndex>,
    module_graph: &ModuleGraph,
    module_render_ids: &HashMap<ModuleHandle, RenderId>,
    source: &mut ReplaceSource,
) {
    let dependency_index =
        dependency_index.expect("Harmony import specifier must have a Dependency Index");
    module_graph
        .module_for_dependency(module_handle, None, dependency_index)
        .expect("Harmony import specifier must have a Module Graph connection");
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
    let Some(used_name) = exports_info.get_used_name(&dep.name) else {
        return;
    };
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
    let Some(used_name) = exports_info.get_used_name("default") else {
        return;
    };
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
    module_handle: ModuleHandle,
    dependency_index: Option<DependencyIndex>,
    module_graph: &ModuleGraph,
    exports_info: &ExportsInfo,
    module_render_ids: &HashMap<ModuleHandle, RenderId>,
    init_fragments: &mut Vec<InitFragment>,
) {
    let dependency_index =
        dependency_index.expect("Harmony re-export must have a Dependency Index");
    let target = module_graph
        .module_for_dependency(module_handle, None, dependency_index)
        .expect("Harmony re-export must have a Module Graph connection");
    if !module_render_ids.contains_key(&target) {
        return;
    }
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
        let Some(used_name) = exports_info.get_used_name(name) else {
            return;
        };
        let expression = export_access_expression(&import_var, &dep.ids);
        push_init_fragment(
            init_fragments,
            InitFragmentStage::HarmonyExport,
            format!(
                "__webpack_require__.d(__webpack_exports__, {{ {}: () => ({expression}) }});\n",
                property_name(used_name),
            ),
        );
    }
}

fn apply_import_dependency(
    dep: &ImportDependency,
    module_handle: ModuleHandle,
    origin_block: Option<AsyncDependenciesBlockIndex>,
    dependency_index: Option<DependencyIndex>,
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    module_render_ids: &HashMap<ModuleHandle, RenderId>,
    runtime_requirements: &mut RuntimeRequirements,
    source: &mut ReplaceSource,
) {
    let block_index = origin_block.expect("Dynamic import must belong to an Async Block");
    let dependency_index = dependency_index.expect("Dynamic import must have a Dependency Index");
    let target = module_graph
        .module_for_dependency(module_handle, Some(block_index), dependency_index)
        .expect("Dynamic import must have a Module Graph connection");
    let target_id = json_render_id(&module_render_ids[&target]);
    let origin = AsyncBlockOrigin {
        module: module_handle,
        block: block_index,
    };
    let expression = if let Some(group_handle) = chunk_graph.block_chunk_group(origin) {
        runtime_requirements.insert(RuntimeRequirement::EnsureChunk);
        let group = &chunk_graph.chunk_groups()[group_handle.index()];
        let chunk_handle = group
            .chunks()
            .first()
            .copied()
            .expect("Async Chunk Group must contain a Chunk");
        let chunk = chunk_graph
            .chunk(chunk_handle)
            .expect("Async Chunk must exist before Dynamic Import generation");
        let chunk_id = json_render_id(chunk.render_id());
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
    simd_json::to_string(value).expect("JavaScript string input must serialize as JSON")
}

fn json_render_id(render_id: &RenderId) -> String {
    match render_id {
        RenderId::String(value) => json_string(value),
        RenderId::Number(value) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs};

    use rspack_sources::{ConcatSource, OriginalSource, ReplacementEnforce, Source};

    use crate::{
        CacheOptions, Compiler, CompilerOptions, ConstDependency, Dependency, Entry, Error,
        ModuleGraph, ModuleHandle, ModuleIdentity, SnapshotOptions, SourceRange,
        cache::{BuildCache, CacheItemFamily, CacheItemWork},
        cache_facade::{CacheIdentifier, CacheKey, CacheNamespace},
        id_assignment::{RenderId, assign_chunk_render_ids, assign_module_render_ids},
        runtime::RuntimeModule,
    };

    use super::{
        AssetRenderKey, AssetRenderKind, AssetRenderManifest, CodeGenerationResult,
        CodeGenerationResults, CodeGenerationSource, ModuleRenderManifest, RenderedRuntimeModule,
        RenderedSource, RuntimeRequirement, emit_asset,
    };
    use crate::code_generation_record::{CodeGenerationRecord, CodeGenerationReplacement};

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
    fn exact_render_hash_covers_generated_source_and_manifest_inputs() {
        let module_handle = ModuleHandle::new(0);
        let module = ModuleRenderManifest {
            module: module_handle,
            render_id: RenderId::String("./src/feature.js".to_string()),
        };
        let render = AssetRenderManifest::AsyncChunk {
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

        let initial = AssetRenderManifest::InitialChunk {
            modules: Vec::new(),
            runtime_modules: vec![RenderedRuntimeModule {
                module: RuntimeModule::GetChunkFilename,
                source: "return feature.js".to_string(),
            }],
            entry_id: RenderId::String("./src/index.js".to_string()),
            chunk_id: RenderId::String("main".to_string()),
        };
        let changed_filename_map = AssetRenderManifest::InitialChunk {
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
            let mut module_graph = ModuleGraph::default();
            let module = module_graph.add_module(ModuleIdentity::new("/project/index.js"));
            module_graph
                .module_mut(module)
                .expect("fixture Module should exist")
                .finish_build(
                    Vec::new(),
                    Vec::new(),
                    vec![Dependency::Const(ConstDependency::new(
                        expression,
                        SourceRange::new(0, 5),
                    ))],
                    "value".to_string(),
                    1,
                );
            let mut chunk_graph =
                crate::build_chunk_graph::build_chunk_graph(&options, &module_graph, &[module]);
            assign_module_render_ids(&options, &module_graph, &mut chunk_graph);
            assign_chunk_render_ids(&options, &module_graph, &mut chunk_graph);
            (module_graph, chunk_graph, module)
        };
        let build_cache = BuildCache::new(CacheOptions::memory(), SnapshotOptions::default());

        let (first_graph, first_chunks, first_module) = build("first");
        let first = super::generate_code_cached(&first_graph, &first_chunks, &build_cache);
        assert_eq!(
            first.results.results[&first_module]
                .source()
                .source()
                .into_string_lossy(),
            "first"
        );

        let (second_graph, second_chunks, second_module) = build("second");
        let second = super::generate_code_cached(&second_graph, &second_chunks, &build_cache);
        assert_eq!(
            second.results.results[&second_module]
                .source()
                .source()
                .into_string_lossy(),
            "second"
        );
        assert_eq!(
            build_cache
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
        let mut module_graph = ModuleGraph::default();
        let module = module_graph.add_module(ModuleIdentity::new("/project/index.js"));
        module_graph
            .module_mut(module)
            .expect("fixture Module should exist")
            .finish_build(Vec::new(), Vec::new(), Vec::new(), "éx".to_string(), 1);
        let mut chunk_graph =
            crate::build_chunk_graph::build_chunk_graph(&options, &module_graph, &[module]);
        assign_module_render_ids(&options, &module_graph, &mut chunk_graph);
        assign_chunk_render_ids(&options, &module_graph, &mut chunk_graph);
        let module_render_ids = HashMap::from([(
            module,
            chunk_graph
                .module_render_id(module)
                .expect("fixture Module should have a Render ID")
                .clone(),
        )]);
        let module_ref = module_graph
            .module(module)
            .expect("fixture Module should exist");
        let input = super::CodeGenerationInput {
            module: module_ref,
            module_graph: &module_graph,
            chunk_graph: &chunk_graph,
            module_render_ids: &module_render_ids,
        };
        let etag = super::code_generation_etag(&input);
        let build_cache = BuildCache::new(CacheOptions::memory(), SnapshotOptions::default());
        let cache = build_cache.code_generations();
        cache.store(
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

        let outcome = super::generate_code_cached(&module_graph, &chunk_graph, &build_cache);
        assert_eq!(
            outcome.results.results[&module]
                .source()
                .source()
                .into_string_lossy(),
            "éx"
        );
        assert!(
            cache
                .get(module_ref.identity(), Some(&etag))
                .expect("regenerated Code Generation Record should be stored")
                .is_compatible_with(module_ref.source())
        );
    }

    #[test]
    fn module_attributable_generation_errors_become_throwing_results() {
        let options = CompilerOptions::new("/project", vec![Entry::new("main", "./index")]);
        let mut module_graph = ModuleGraph::default();
        let module = module_graph.add_module(ModuleIdentity::new("/project/index.js"));
        module_graph
            .module_mut(module)
            .expect("fixture Module should exist")
            .finish_build(
                Vec::new(),
                Vec::new(),
                vec![Dependency::Const(ConstDependency::new(
                    "replacement",
                    SourceRange::new(0, 99),
                ))],
                "value".to_string(),
                1,
            );
        let mut chunk_graph =
            crate::build_chunk_graph::build_chunk_graph(&options, &module_graph, &[module]);
        assign_module_render_ids(&options, &module_graph, &mut chunk_graph);
        assign_chunk_render_ids(&options, &module_graph, &mut chunk_graph);

        let build_cache = BuildCache::new(CacheOptions::memory(), SnapshotOptions::default());
        for outcome in [
            super::generate_code_cached(&module_graph, &chunk_graph, &build_cache),
            super::generate_code_cached(&module_graph, &chunk_graph, &build_cache),
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
            build_cache
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
                super::generate_module_code(input)
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
                    super::generate_module_code(input)
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
