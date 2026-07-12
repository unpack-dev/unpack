// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/asset/AssetModulesPlugin.js

use crate::{
    ModuleType,
    asset::{asset_generator, asset_parser},
    code_generation::{RenderManifestContent, RenderManifestContext, RenderManifestEntry},
    compiler::CompilerHookSet,
    normal_module_factory::{ModuleSourceKind, ModuleTypeRegistration},
};

pub(crate) struct AssetModulesPlugin;

impl AssetModulesPlugin {
    pub(crate) fn apply(&self, hooks: &mut CompilerHookSet) {
        hooks.compilation.tap("AssetModulesPlugin", |hooks| {
            for module_type in [
                ModuleType::Asset,
                ModuleType::AssetResource,
                ModuleType::AssetInline,
                ModuleType::AssetSource,
            ] {
                hooks.normal_module_factory_hooks.register(
                    module_type,
                    ModuleTypeRegistration {
                        parser: asset_parser::parse,
                        generator: asset_generator::generate,
                        source_kind: ModuleSourceKind::Binary,
                        side_effect_free: true,
                    },
                );
            }
            hooks.render_manifest.tap(render_manifest);
        });
    }
}

fn render_manifest(context: RenderManifestContext<'_>) -> Vec<RenderManifestEntry> {
    asset_generator::render_resource_assets(context.module_graph, context.chunk_graph)
        .into_iter()
        .map(|asset| RenderManifestEntry {
            filename: asset.filename.clone(),
            render: RenderManifestContent::Asset(asset),
        })
        .collect()
}
