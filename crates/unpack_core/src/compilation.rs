use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{
    Asset, ChunkGraph, CompilerOptions, Error, ModuleGraph, ModuleId, Result, UnpackResolver,
    code_generation,
    make::{self, MakeState},
};

#[derive(Debug, Clone)]
pub struct Compilation {
    options: CompilerOptions,
    resolver: UnpackResolver,
    module_graph: ModuleGraph,
    chunk_graph: ChunkGraph,
    assets: Vec<Asset>,
    entries: Vec<ModuleId>,
    errors: Vec<Error>,
}

impl Compilation {
    pub(crate) fn new(options: CompilerOptions, resolver: UnpackResolver) -> Self {
        Self {
            options,
            resolver,
            module_graph: ModuleGraph::default(),
            chunk_graph: ChunkGraph::default(),
            assets: Vec::new(),
            entries: Vec::new(),
            errors: Vec::new(),
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

    pub async fn make(&mut self) -> Result<()> {
        let state = Arc::new(Mutex::new(MakeState::default()));
        let result = make::run(&self.options, self.resolver.clone(), Arc::clone(&state)).await;

        let mut state = state.lock().await;
        self.module_graph = std::mem::take(&mut state.module_graph);
        self.entries = std::mem::take(&mut state.entries).into_values().collect();
        self.errors = std::mem::take(&mut state.errors);

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
