use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{
    CompilerOptions, Error, ModuleGraph, ModuleId, Result, UnpackResolver,
    make::{self, MakeState},
};

#[derive(Debug, Clone)]
pub struct Compilation {
    options: CompilerOptions,
    resolver: UnpackResolver,
    module_graph: ModuleGraph,
    entries: Vec<ModuleId>,
    errors: Vec<Error>,
}

impl Compilation {
    pub(crate) fn new(options: CompilerOptions, resolver: UnpackResolver) -> Self {
        Self {
            options,
            resolver,
            module_graph: ModuleGraph::default(),
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
        self.entries = std::mem::take(&mut state.entries);
        self.errors = std::mem::take(&mut state.errors);

        result
    }
}
