// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/ModuleGraph.js

use rustc_hash::FxHashMap;

use crate::index_vec::IndexVec;
use crate::{
    ExportsInfo, Module, ModuleGraphConnection, ModuleGraphConnectionHandle, ModuleHandle,
};

mod building_module_graph;

pub(crate) use building_module_graph::BuildingModuleGraph;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ModuleGraph {
    storage: ModuleGraphStorage<Module>,
    exports_info: IndexVec<ModuleHandle, ExportsInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModuleGraphStorage<M> {
    modules: Vec<M>,
    connections: Vec<ModuleGraphConnection>,
    outgoing: IndexVec<ModuleHandle, Vec<ModuleGraphConnectionHandle>>,
    incoming: IndexVec<ModuleHandle, Vec<ModuleGraphConnectionHandle>>,
    outgoing_by_location:
        IndexVec<ModuleHandle, FxHashMap<DependencyLocation, ModuleGraphConnectionHandle>>,
}

impl<M> Default for ModuleGraphStorage<M> {
    fn default() -> Self {
        Self {
            modules: Vec::new(),
            connections: Vec::new(),
            outgoing: IndexVec::default(),
            incoming: IndexVec::default(),
            outgoing_by_location: IndexVec::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DependencyIndex(usize);

impl DependencyIndex {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AsyncDependenciesBlockIndex(usize);

impl AsyncDependenciesBlockIndex {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

impl ModuleGraph {
    pub fn exports_info(&self, handle: ModuleHandle) -> &ExportsInfo {
        &self.exports_info[handle]
    }

    pub(crate) fn exports_info_mut(&mut self, handle: ModuleHandle) -> &mut ExportsInfo {
        &mut self.exports_info[handle]
    }

    pub fn modules(&self) -> &[Module] {
        &self.storage.modules
    }

    pub fn module(&self, handle: ModuleHandle) -> Option<&Module> {
        self.storage.modules.get(handle.index())
    }

    pub fn connections(&self) -> &[ModuleGraphConnection] {
        &self.storage.connections
    }

    pub(crate) fn connections_mut(&mut self) -> &mut [ModuleGraphConnection] {
        &mut self.storage.connections
    }

    pub(crate) fn update_connection_module(
        &mut self,
        handle: ModuleGraphConnectionHandle,
        module: ModuleHandle,
    ) {
        let connection = &mut self.storage.connections[handle.index()];
        if connection.module == module {
            return;
        }
        self.storage.incoming[connection.module].retain(|candidate| *candidate != handle);
        self.storage.incoming[module].push(handle);
        connection.module = module;
    }

    pub(crate) fn connection_mut(
        &mut self,
        handle: ModuleGraphConnectionHandle,
    ) -> &mut ModuleGraphConnection {
        &mut self.storage.connections[handle.index()]
    }

    pub fn outgoing_connections(
        &self,
        module: ModuleHandle,
    ) -> impl Iterator<Item = &ModuleGraphConnection> {
        self.storage.outgoing[module]
            .iter()
            .map(|connection_handle| &self.storage.connections[connection_handle.index()])
    }

    pub fn incoming_connections(
        &self,
        module: ModuleHandle,
    ) -> impl Iterator<Item = &ModuleGraphConnection> {
        self.storage.incoming[module]
            .iter()
            .map(|connection_handle| &self.storage.connections[connection_handle.index()])
    }

    pub fn module_for_dependency(
        &self,
        origin_module: ModuleHandle,
        origin_block: Option<AsyncDependenciesBlockIndex>,
        dependency_index: DependencyIndex,
    ) -> Option<ModuleHandle> {
        let location = DependencyLocation {
            block: origin_block,
            dependency_index,
        };
        self.storage
            .outgoing_by_location
            .get(origin_module)?
            .get(&location)
            .map(|connection_handle| self.storage.connections[connection_handle.index()].module)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DependencyLocation {
    block: Option<AsyncDependenciesBlockIndex>,
    dependency_index: DependencyIndex,
}
