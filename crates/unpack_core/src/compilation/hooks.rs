// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/Compilation.js

use std::{future::Future, pin::Pin, sync::Arc};

use super::Compilation;
use crate::parser::JavascriptParserHookSet;

type AsyncTap = Arc<
    dyn for<'a> Fn(&'a mut Compilation) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
        + Send
        + Sync,
>;
type SyncTap = Arc<dyn Fn(&mut Compilation) + Send + Sync>;
type RenderManifestTap = for<'a> fn(
    crate::code_generation::RenderManifestContext<'a>,
) -> Vec<crate::code_generation::RenderManifestEntry>;

#[derive(Default, Clone)]
pub(crate) struct AsyncCompilationHook {
    taps: Vec<(&'static str, AsyncTap)>,
}

impl AsyncCompilationHook {
    pub fn tap(
        &mut self,
        name: &'static str,
        tap: impl for<'a> Fn(&'a mut Compilation) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
        + Send
        + Sync
        + 'static,
    ) {
        self.taps.push((name, Arc::new(tap)));
    }

    pub async fn call(&self, compilation: &mut Compilation) {
        for (_, tap) in &self.taps {
            tap(compilation).await;
        }
    }
}

#[derive(Default, Clone)]
pub(crate) struct SyncCompilationHook {
    taps: Vec<(&'static str, SyncTap)>,
}

impl SyncCompilationHook {
    pub fn tap(
        &mut self,
        name: &'static str,
        tap: impl Fn(&mut Compilation) + Send + Sync + 'static,
    ) {
        self.taps.push((name, Arc::new(tap)));
    }

    pub fn call(&self, compilation: &mut Compilation) {
        for (_, tap) in &self.taps {
            tap(compilation);
        }
    }
}

#[derive(Default, Clone)]
pub(crate) struct CompilationHookSet {
    pub normal_module_factory_hooks: crate::normal_module_factory::ModuleTypeRegistry,
    pub render_manifest: RenderManifestHook,
    pub javascript_parser: JavascriptParserHookSet,
    pub finish_modules: AsyncCompilationHook,
    pub optimize_dependencies: SyncCompilationHook,
    pub optimize_chunk_modules: SyncCompilationHook,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct RenderManifestHook {
    taps: Vec<RenderManifestTap>,
}

impl RenderManifestHook {
    pub(crate) fn tap(&mut self, tap: RenderManifestTap) {
        self.taps.push(tap);
    }

    pub(crate) fn call(
        &self,
        context: crate::code_generation::RenderManifestContext<'_>,
    ) -> Vec<crate::code_generation::RenderManifestEntry> {
        self.taps.iter().flat_map(|tap| tap(context)).collect()
    }
}

impl std::fmt::Debug for CompilationHookSet {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CompilationHookSet")
            .field(
                "normal_module_factory_hooks",
                &self.normal_module_factory_hooks,
            )
            .field("render_manifest", &self.render_manifest)
            .field("javascript_parser", &self.javascript_parser)
            .field("finish_modules_taps", &self.finish_modules.taps.len())
            .field(
                "optimize_dependencies_taps",
                &self.optimize_dependencies.taps.len(),
            )
            .field(
                "optimize_chunk_modules_taps",
                &self.optimize_chunk_modules.taps.len(),
            )
            .finish()
    }
}
