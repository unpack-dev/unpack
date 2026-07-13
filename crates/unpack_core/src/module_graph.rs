// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/ModuleGraph.js

use rustc_hash::FxHashMap;

use crate::index_vec::IndexVec;
use crate::{
    Dependency, Module, ModuleGraphConnection, ModuleGraphConnectionHandle,
    ModuleGraphConnectionState, ModuleHandle, ModuleIdentity,
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ModuleGraph {
    modules: Vec<Module>,
    connections: Vec<ModuleGraphConnection>,
    outgoing: IndexVec<ModuleHandle, Vec<ModuleGraphConnectionHandle>>,
    incoming: IndexVec<ModuleHandle, Vec<ModuleGraphConnectionHandle>>,
    outgoing_by_location:
        IndexVec<ModuleHandle, FxHashMap<DependencyLocation, ModuleGraphConnectionHandle>>,
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
    pub(crate) fn add_module(&mut self, identity: ModuleIdentity) -> ModuleHandle {
        let handle = ModuleHandle::new(self.modules.len());
        self.modules.push(Module::new(handle, identity));
        let outgoing_handle = self.outgoing.push(Vec::new());
        let incoming_handle = self.incoming.push(Vec::new());
        let location_handle = self.outgoing_by_location.push(FxHashMap::default());
        debug_assert_eq!(outgoing_handle, handle);
        debug_assert_eq!(incoming_handle, handle);
        debug_assert_eq!(location_handle, handle);
        handle
    }

    pub(crate) fn module_mut(&mut self, handle: ModuleHandle) -> Option<&mut Module> {
        self.modules.get_mut(handle.index())
    }

    pub(crate) fn connect(
        &mut self,
        origin_module: Option<ModuleHandle>,
        origin_block: Option<AsyncDependenciesBlockIndex>,
        origin_dependency_index: Option<DependencyIndex>,
        dependency: Dependency,
        module: ModuleHandle,
    ) {
        let connection_handle = ModuleGraphConnectionHandle::new(self.connections.len());
        self.connections.push(ModuleGraphConnection {
            handle: connection_handle,
            origin_module,
            origin_block,
            origin_dependency_index,
            dependency,
            resolved_module: module,
            module,
            state: ModuleGraphConnectionState::Active,
        });
        if let Some(origin_module) = origin_module {
            self.outgoing[origin_module].push(connection_handle);
            if let Some(dependency_index) = origin_dependency_index {
                self.outgoing_by_location[origin_module].insert(
                    DependencyLocation {
                        block: origin_block,
                        dependency_index,
                    },
                    connection_handle,
                );
            }
        }
        self.incoming[module].push(connection_handle);
    }

    pub fn modules(&self) -> &[Module] {
        &self.modules
    }

    pub fn module(&self, handle: ModuleHandle) -> Option<&Module> {
        self.modules.get(handle.index())
    }

    pub fn connections(&self) -> &[ModuleGraphConnection] {
        &self.connections
    }

    pub(crate) fn connections_mut(&mut self) -> &mut [ModuleGraphConnection] {
        &mut self.connections
    }

    pub(crate) fn update_connection_module(
        &mut self,
        handle: ModuleGraphConnectionHandle,
        module: ModuleHandle,
    ) {
        let connection = &mut self.connections[handle.index()];
        if connection.module == module {
            return;
        }
        self.incoming[connection.module].retain(|candidate| *candidate != handle);
        self.incoming[module].push(handle);
        connection.module = module;
    }

    pub(crate) fn connection_mut(
        &mut self,
        handle: ModuleGraphConnectionHandle,
    ) -> &mut ModuleGraphConnection {
        &mut self.connections[handle.index()]
    }

    pub fn outgoing_connections(
        &self,
        module: ModuleHandle,
    ) -> impl Iterator<Item = &ModuleGraphConnection> {
        self.outgoing[module]
            .iter()
            .map(|connection_handle| &self.connections[connection_handle.index()])
    }

    pub fn incoming_connections(
        &self,
        module: ModuleHandle,
    ) -> impl Iterator<Item = &ModuleGraphConnection> {
        self.incoming[module]
            .iter()
            .map(|connection_handle| &self.connections[connection_handle.index()])
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
        self.outgoing_by_location
            .get(origin_module)?
            .get(&location)
            .map(|connection_handle| self.connections[connection_handle.index()].module)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DependencyLocation {
    block: Option<AsyncDependenciesBlockIndex>,
    dependency_index: DependencyIndex,
}

#[cfg(test)]
mod tests {
    use super::ModuleGraph;
    use crate::ModuleIdentity;

    #[test]
    fn adding_a_module_initializes_its_connection_indices() {
        let mut module_graph = ModuleGraph::default();
        let module = module_graph.add_module(ModuleIdentity::new("/project/src/index.js"));

        assert_eq!(module_graph.outgoing_connections(module).count(), 0);
        assert_eq!(module_graph.incoming_connections(module).count(), 0);
    }
}
