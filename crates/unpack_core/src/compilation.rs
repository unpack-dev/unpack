use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use tokio::sync::Mutex;

use crate::{
    Asset, ChunkGraph, CompilerOptions, Error, InfrastructureLogEvent, InfrastructureLogLevel,
    ModuleGraph, ModuleId, Result, UnpackResolver,
    build_cache::BuildCache,
    code_generation::{self, AssetRenderPlan, CodeGenerationResults, RenderedSource},
    make::{self, MakeState},
    snapshot::FileSystemInfo,
};
use tracing::Instrument;

#[derive(Debug, Clone)]
pub struct Compilation {
    options: CompilerOptions,
    resolver: UnpackResolver,
    build_cache: BuildCache,
    module_graph: ModuleGraph,
    chunk_graph: ChunkGraph,
    code_generation_results: Option<CodeGenerationResults>,
    asset_render_plan: Option<AssetRenderPlan>,
    rendered_asset_sources: Option<Vec<RenderedSource>>,
    assets: Vec<Asset>,
    entries: Vec<ModuleId>,
    errors: Vec<Error>,
    watch_dependencies: WatchDependencies,
    infrastructure_log_events: Vec<InfrastructureLogEvent>,
    file_system_info: FileSystemInfo,
}

impl Compilation {
    pub(crate) fn new(
        options: CompilerOptions,
        resolver: UnpackResolver,
        build_cache: BuildCache,
    ) -> Self {
        let file_system_info = FileSystemInfo::from_snapshot_options(&options.snapshot);
        Self {
            options,
            resolver,
            build_cache,
            module_graph: ModuleGraph::default(),
            chunk_graph: ChunkGraph::default(),
            code_generation_results: None,
            asset_render_plan: None,
            rendered_asset_sources: None,
            assets: Vec::new(),
            entries: Vec::new(),
            errors: Vec::new(),
            watch_dependencies: WatchDependencies::default(),
            infrastructure_log_events: Vec::new(),
            file_system_info,
        }
    }

    pub fn options(&self) -> &CompilerOptions {
        &self.options
    }

    pub fn module_graph(&self) -> &ModuleGraph {
        &self.module_graph
    }

    pub fn chunk_graph(&self) -> &ChunkGraph {
        &self.chunk_graph
    }

    pub fn assets(&self) -> &[Asset] {
        &self.assets
    }

    pub fn entries(&self) -> &[ModuleId] {
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
                self.build_cache.clone(),
                self.file_system_info.clone(),
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
            self.invalidate_generated_work();

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
        self.chunk_graph = ChunkGraph::build(&self.options, &self.module_graph, &self.entries);
        self.invalidate_generated_work();
        self.log_infrastructure(
            InfrastructureLogLevel::Verbose,
            "unpack.Compilation",
            "chunk graph build completed",
        );
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
        self.render_asset_sources();
        self.emit_assets();
        self.log_infrastructure(
            InfrastructureLogLevel::Verbose,
            "unpack.Compilation",
            "asset creation completed",
        );
    }

    pub fn create_asset_render_manifest(&mut self) {
        if self.code_generation_results.is_none() {
            self.code_generation();
        }
        self.asset_render_plan = Some(code_generation::create_render_manifest(
            &self.chunk_graph,
            &self.entries,
            self.code_generation_results
                .as_ref()
                .expect("code generation results should exist before render manifest creation"),
        ));
    }

    pub fn render_asset_sources(&mut self) {
        if self.asset_render_plan.is_none() {
            self.create_asset_render_manifest();
        }
        self.rendered_asset_sources = Some(code_generation::render_asset_sources(
            self.asset_render_plan
                .as_ref()
                .expect("render plan should exist before Asset rendering")
                .manifest(),
            self.code_generation_results
                .as_ref()
                .expect("code generation results should exist before Asset rendering"),
        ));
    }

    pub fn emit_assets(&mut self) {
        if self.rendered_asset_sources.is_none() {
            self.render_asset_sources();
        }
        self.assets = code_generation::create_assets(
            &self.options,
            self.asset_render_plan
                .as_ref()
                .expect("render plan should exist before Asset emission"),
            self.rendered_asset_sources
                .as_ref()
                .expect("rendered sources should exist before Asset emission"),
        );
    }

    pub fn code_generation(&mut self) {
        let span = tracing::trace_span!("Compilation::code_generation");
        let _enter = span.enter();
        self.code_generation_results = Some(code_generation::generate_code(
            &self.options,
            &self.module_graph,
            &self.chunk_graph,
        ));
        self.asset_render_plan = None;
        self.rendered_asset_sources = None;
    }

    fn invalidate_generated_work(&mut self) {
        self.code_generation_results = None;
        self.asset_render_plan = None;
        self.rendered_asset_sources = None;
        self.assets.clear();
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
