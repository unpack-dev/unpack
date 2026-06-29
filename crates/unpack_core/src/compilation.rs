use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use tokio::sync::Mutex;

use crate::{
    Asset, ChunkGraph, CompilerOptions, Error, ModuleGraph, ModuleId, Result, UnpackResolver,
    build_cache::BuildCache,
    code_generation,
    make::{self, MakeState},
    snapshot::FileSystemInfo,
};

#[derive(Debug, Clone)]
pub struct Compilation {
    options: CompilerOptions,
    resolver: UnpackResolver,
    build_cache: BuildCache,
    module_graph: ModuleGraph,
    chunk_graph: ChunkGraph,
    assets: Vec<Asset>,
    entries: Vec<ModuleId>,
    errors: Vec<Error>,
    watch_dependencies: WatchDependencies,
    file_system_info: FileSystemInfo,
}

impl Compilation {
    pub(crate) fn new(
        options: CompilerOptions,
        resolver: UnpackResolver,
        build_cache: BuildCache,
    ) -> Self {
        Self {
            options,
            resolver,
            build_cache,
            module_graph: ModuleGraph::default(),
            chunk_graph: ChunkGraph::default(),
            assets: Vec::new(),
            entries: Vec::new(),
            errors: Vec::new(),
            watch_dependencies: WatchDependencies::default(),
            file_system_info: FileSystemInfo::new(),
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

    pub async fn make(&mut self) -> Result<()> {
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
            files: std::mem::take(&mut state.file_dependencies),
            contexts: std::mem::take(&mut state.context_dependencies),
            missing: std::mem::take(&mut state.missing_dependencies),
        };

        result
    }

    pub fn build_chunk_graph(&mut self) {
        self.chunk_graph = ChunkGraph::build(&self.options, &self.module_graph, &self.entries);
    }

    pub fn create_assets(&mut self) {
        self.assets = code_generation::create_assets(
            &self.options,
            &self.module_graph,
            &self.chunk_graph,
            &self.entries,
        );
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
