// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/javascript/JavascriptModulesPlugin.js

use crate::{
    ChunkGroupKind, ModuleType,
    code_generation::{
        JavascriptRenderManifest, RenderManifestContent, RenderManifestContext,
        RenderManifestEntry, RenderedRuntimeModule, module_render_manifest,
    },
    compiler::CompilerHookSet,
    javascript::{javascript_generator, javascript_parser},
    normal_module_factory::{ModuleSourceKind, ModuleTypeRegistration},
    output_filename::resolve_chunk_filename,
    runtime::RuntimeModuleContext,
};

pub(crate) struct JavascriptModulesPlugin;

impl JavascriptModulesPlugin {
    pub(crate) fn apply(&self, hooks: &mut CompilerHookSet) {
        hooks.compilation.tap("JavascriptModulesPlugin", |hooks| {
            hooks.normal_module_factory_hooks.register(
                ModuleType::JavaScriptAuto,
                ModuleTypeRegistration {
                    parser: javascript_parser::parse,
                    generator: javascript_generator::generate,
                    source_kind: ModuleSourceKind::Text,
                    side_effect_free: false,
                },
            );
            hooks.render_manifest.tap(render_manifest);
        });
    }
}

fn render_manifest(context: RenderManifestContext<'_>) -> Vec<RenderManifestEntry> {
    let mut entries = Vec::new();
    for (entry_index, group_handle) in context
        .chunk_graph
        .entrypoints()
        .iter()
        .copied()
        .enumerate()
    {
        let group = &context.chunk_graph.chunk_groups()[group_handle.index()];
        let chunk_handle = group
            .chunks()
            .first()
            .copied()
            .expect("Entrypoint must contain a Chunk before manifest creation");
        let chunk = context
            .chunk_graph
            .chunk(chunk_handle)
            .expect("Entrypoint Chunk must exist before manifest creation");
        let entry_module = context
            .entries
            .get(entry_index)
            .copied()
            .expect("Entrypoint must have an Entry Module before manifest creation");
        context
            .chunk_graph
            .runtime_tree_requirements(group_handle)
            .expect("Runtime Requirements must be processed before manifest creation");
        let runtime_context = RuntimeModuleContext {
            chunk_graph: context.chunk_graph,
            runtime_chunk: chunk_handle,
        };
        let runtime_modules = context
            .chunk_graph
            .runtime_modules(chunk_handle)
            .iter()
            .map(|module| RenderedRuntimeModule {
                module: *module,
                source: module.generate(&runtime_context),
            })
            .collect();
        entries.push(RenderManifestEntry {
            filename: resolve_chunk_filename(chunk),
            render: RenderManifestContent::JavaScript(JavascriptRenderManifest::InitialChunk {
                modules: module_render_manifest(
                    context.chunk_graph,
                    chunk,
                    context.code_generation_results,
                ),
                runtime_modules,
                entry_id: context
                    .code_generation_results
                    .module_render_id(entry_module)
                    .expect("Entry Module must have a Render ID before manifest creation")
                    .clone(),
                chunk_id: chunk.render_id().clone(),
            }),
        });
    }

    for chunk in context.chunk_graph.chunks() {
        let is_initial = chunk.groups().iter().any(|group_handle| {
            matches!(
                context.chunk_graph.chunk_groups()[group_handle.index()].kind(),
                ChunkGroupKind::Entrypoint { .. }
            )
        });
        if is_initial {
            continue;
        }
        entries.push(RenderManifestEntry {
            filename: resolve_chunk_filename(chunk),
            render: RenderManifestContent::JavaScript(JavascriptRenderManifest::AsyncChunk {
                modules: module_render_manifest(
                    context.chunk_graph,
                    chunk,
                    context.code_generation_results,
                ),
                chunk_id: chunk.render_id().clone(),
            }),
        });
    }
    entries
}
