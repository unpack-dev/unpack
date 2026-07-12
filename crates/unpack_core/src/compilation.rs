// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/Compilation.js

use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use tokio::sync::Mutex;

use crate::{
    Asset, ChunkGraph, CompilerOptions, Error, InfrastructureLogEvent, InfrastructureLogLevel,
    ModuleGraph, ModuleHandle, Result, UnpackResolver,
    build_chunk_graph::build_chunk_graph_with_cache,
    cache::Cache,
    code_generation::{self, CodeGenerationResults, RenderManifest},
    id_assignment::{assign_chunk_render_ids, assign_module_render_ids},
    make::{self, MakeState},
    module_computation_cache::ModuleComputationCache,
    snapshot::FileSystemInfo,
};
use tracing::Instrument;

mod hooks;
pub(crate) use hooks::{CompilationHookSet, RenderManifestHook};

#[derive(Debug, Clone)]
pub struct Compilation {
    options: CompilerOptions,
    resolver: UnpackResolver,
    cache: Cache,
    module_computation_cache: Option<ModuleComputationCache>,
    module_graph: ModuleGraph,
    chunk_graph: ChunkGraph,
    render_ids_assigned: bool,
    code_generation_results: Option<CodeGenerationResults>,
    asset_render_manifest: Option<RenderManifest>,
    assets: Vec<Asset>,
    entries: Vec<ModuleHandle>,
    errors: Vec<Error>,
    watch_dependencies: WatchDependencies,
    infrastructure_log_events: Vec<InfrastructureLogEvent>,
    file_system_info: FileSystemInfo,
    hooks: CompilationHookSet,
}

impl Compilation {
    pub(crate) fn new(
        options: CompilerOptions,
        resolver: UnpackResolver,
        cache: Cache,
        module_computation_cache: Option<ModuleComputationCache>,
        hooks: CompilationHookSet,
    ) -> Self {
        let file_system_info = FileSystemInfo::from_snapshot_options(&options.snapshot);
        Self {
            options,
            resolver,
            cache,
            module_computation_cache,
            module_graph: ModuleGraph::default(),
            chunk_graph: ChunkGraph::default(),
            render_ids_assigned: false,
            code_generation_results: None,
            asset_render_manifest: None,
            assets: Vec::new(),
            entries: Vec::new(),
            errors: Vec::new(),
            watch_dependencies: WatchDependencies::default(),
            infrastructure_log_events: Vec::new(),
            file_system_info,
            hooks,
        }
    }

    pub fn options(&self) -> &CompilerOptions {
        &self.options
    }

    pub fn module_graph(&self) -> &ModuleGraph {
        &self.module_graph
    }

    pub(crate) fn module_graph_mut(&mut self) -> &mut ModuleGraph {
        &mut self.module_graph
    }

    /// Temporarily detaches the module graph for a host hook that takes
    /// ownership of it. The caller must restore the graph before compilation
    /// continues.
    pub fn take_module_graph(&mut self) -> ModuleGraph {
        std::mem::take(&mut self.module_graph)
    }

    /// Restores a module graph previously detached by `take_module_graph`.
    pub fn restore_module_graph(&mut self, module_graph: ModuleGraph) {
        debug_assert!(self.module_graph.modules().is_empty());
        self.module_graph = module_graph;
    }

    pub(crate) fn module_computation_cache(&self) -> Option<&ModuleComputationCache> {
        self.module_computation_cache.as_ref()
    }

    pub fn into_graphs(self) -> (ModuleGraph, ChunkGraph) {
        (self.module_graph, self.chunk_graph)
    }

    pub fn chunk_graph(&self) -> &ChunkGraph {
        &self.chunk_graph
    }

    pub fn assets(&self) -> &[Asset] {
        &self.assets
    }

    pub fn entries(&self) -> &[ModuleHandle] {
        &self.entries
    }

    pub fn errors(&self) -> &[Error] {
        &self.errors
    }

    pub fn watch_dependencies(&self) -> &WatchDependencies {
        &self.watch_dependencies
    }

    pub fn infrastructure_log_events(&self) -> &[InfrastructureLogEvent] {
        &self.infrastructure_log_events
    }

    pub(crate) fn extend_infrastructure_log_events(
        &mut self,
        events: impl IntoIterator<Item = InfrastructureLogEvent>,
    ) {
        self.infrastructure_log_events.extend(events);
    }

    pub async fn make(&mut self) -> Result<()> {
        async {
            self.log_infrastructure(
                InfrastructureLogLevel::Verbose,
                "unpack.Compilation",
                "make started",
            );
            let state = Arc::new(Mutex::new(MakeState::default()));
            let result = make::run(
                &self.options,
                self.resolver.clone(),
                self.cache.clone(),
                self.file_system_info.clone(),
                self.hooks.normal_module_factory_hooks.clone(),
                self.hooks.javascript_parser.clone(),
                Arc::clone(&state),
            )
            .await;

            let mut state = state.lock().await;
            self.module_graph = std::mem::take(&mut state.module_graph);
            self.entries = std::mem::take(&mut state.entries).into_values().collect();
            self.errors = std::mem::take(&mut state.errors);
            self.watch_dependencies = WatchDependencies {
                files: std::mem::take(&mut state.file_dependencies)
                    .into_iter()
                    .collect(),
                contexts: std::mem::take(&mut state.context_dependencies)
                    .into_iter()
                    .collect(),
                missing: std::mem::take(&mut state.missing_dependencies)
                    .into_iter()
                    .collect(),
            };

            if result.is_ok()
                && let Some(cache) = &self.module_computation_cache
            {
                cache.prepare(&self.module_graph);
            }

            if result.is_ok() {
                self.hooks.clone().finish_modules.call(self).await;
            }

            if result.is_ok() {
                self.log_infrastructure(
                    InfrastructureLogLevel::Verbose,
                    "unpack.Compilation",
                    "make completed",
                );
            }

            result
        }
        .instrument(tracing::trace_span!("Compilation::make"))
        .await
    }

    pub fn build_chunk_graph(&mut self) {
        let span = tracing::trace_span!("Compilation::build_chunk_graph");
        let _enter = span.enter();
        self.log_infrastructure(
            InfrastructureLogLevel::Verbose,
            "unpack.Compilation",
            "chunk graph build started",
        );
        self.chunk_graph = build_chunk_graph_with_cache(
            &self.options,
            &self.module_graph,
            &self.entries,
            self.module_computation_cache.as_ref(),
        );
        self.render_ids_assigned = false;
        self.log_infrastructure(
            InfrastructureLogLevel::Verbose,
            "unpack.Compilation",
            "chunk graph build completed",
        );
    }

    pub(crate) fn assign_render_ids(&mut self) {
        self.assign_module_ids();
        self.assign_chunk_ids();
        self.render_ids_assigned = true;
    }

    pub fn seal(&mut self) {
        self.hooks.clone().optimize_dependencies.call(self);
        self.build_chunk_graph();
        self.assign_render_ids();
        self.code_generation();
        self.process_runtime_requirements();
        self.create_assets();
    }

    fn assign_module_ids(&mut self) {
        assign_module_render_ids(&self.options, &self.module_graph, &mut self.chunk_graph);
    }

    fn assign_chunk_ids(&mut self) {
        assign_chunk_render_ids(&self.options, &self.module_graph, &mut self.chunk_graph);
    }

    pub fn create_assets(&mut self) {
        let span = tracing::trace_span!("Compilation::create_assets");
        let _enter = span.enter();
        self.log_infrastructure(
            InfrastructureLogLevel::Verbose,
            "unpack.Compilation",
            "asset creation started",
        );
        self.create_asset_render_manifest();
        self.render_assets();
        self.log_infrastructure(
            InfrastructureLogLevel::Verbose,
            "unpack.Compilation",
            "asset creation completed",
        );
    }

    pub fn create_asset_render_manifest(&mut self) {
        let code_generation_results = self
            .code_generation_results
            .as_ref()
            .expect("code generation results should exist before render manifest creation");
        self.asset_render_manifest = Some(code_generation::create_render_manifest(
            code_generation::RenderManifestContext {
                module_graph: &self.module_graph,
                chunk_graph: &self.chunk_graph,
                entries: &self.entries,
                code_generation_results,
            },
            &self.hooks.render_manifest,
        ));
    }

    pub fn render_assets(&mut self) {
        self.assets = code_generation::render_assets(
            &self.options,
            &self.cache,
            self.asset_render_manifest
                .as_ref()
                .expect("render manifest should exist before Asset rendering"),
            self.code_generation_results
                .as_ref()
                .expect("code generation results should exist before Asset rendering"),
        );
    }

    pub fn code_generation(&mut self) {
        let span = tracing::trace_span!("Compilation::code_generation");
        let _enter = span.enter();
        assert!(
            self.render_ids_assigned,
            "Render IDs must be assigned before code generation"
        );
        let outcome = code_generation::generate_code_cached(
            &self.module_graph,
            &self.chunk_graph,
            &self.cache,
            &self.hooks.normal_module_factory_hooks,
        );
        self.errors.extend(outcome.errors);
        self.code_generation_results = Some(outcome.results);
        self.asset_render_manifest = None;
    }

    pub(crate) fn process_runtime_requirements(&mut self) {
        let requirements = self
            .code_generation_results
            .as_ref()
            .expect("code generation results should exist before Runtime Requirements processing")
            .runtime_requirements()
            .map(|(module, requirements)| (module, *requirements))
            .collect::<Vec<_>>();
        self.chunk_graph.process_runtime_requirements(requirements);
    }

    fn log_infrastructure(
        &mut self,
        level: InfrastructureLogLevel,
        name: impl Into<String>,
        message: impl Into<String>,
    ) {
        if self.options.infrastructure_logging.enabled(level) {
            self.infrastructure_log_events
                .push(InfrastructureLogEvent::new(level, name, message));
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WatchDependencies {
    files: BTreeSet<PathBuf>,
    contexts: BTreeSet<PathBuf>,
    missing: BTreeSet<PathBuf>,
}

impl WatchDependencies {
    pub fn files(&self) -> &BTreeSet<PathBuf> {
        &self.files
    }

    pub fn contexts(&self) -> &BTreeSet<PathBuf> {
        &self.contexts
    }

    pub fn missing(&self) -> &BTreeSet<PathBuf> {
        &self.missing
    }
}
