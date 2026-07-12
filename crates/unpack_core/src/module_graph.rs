use std::collections::HashMap;

use crate::{
    Dependency, Module, ModuleGraphConnection, ModuleGraphConnectionHandle,
    ModuleGraphConnectionState, ModuleHandle, ModuleIdentity,
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ModuleGraph {
    modules: Vec<Module>,
    connections: Vec<ModuleGraphConnection>,
    outgoing: Vec<Vec<ModuleGraphConnectionHandle>>,
    incoming: Vec<Vec<ModuleGraphConnectionHandle>>,
    outgoing_by_location: Vec<HashMap<DependencyLocation, ModuleGraphConnectionHandle>>,
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
        self.outgoing.push(Vec::new());
        self.incoming.push(Vec::new());
        self.outgoing_by_location.push(HashMap::new());
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
            module,
            state: ModuleGraphConnectionState::Active,
        });
        if let Some(origin_module) = origin_module {
            self.outgoing[origin_module.index()].push(connection_handle);
            if let Some(dependency_index) = origin_dependency_index {
                self.outgoing_by_location[origin_module.index()].insert(
                    DependencyLocation {
                        block: origin_block,
                        dependency_index,
                    },
                    connection_handle,
                );
            }
        }
        self.incoming[module.index()].push(connection_handle);
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
        self.incoming[connection.module.index()].retain(|candidate| *candidate != handle);
        self.incoming[module.index()].push(handle);
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
        self.outgoing[module.index()]
            .iter()
            .map(|connection_handle| &self.connections[connection_handle.index()])
    }

    pub fn incoming_connections(
        &self,
        module: ModuleHandle,
    ) -> impl Iterator<Item = &ModuleGraphConnection> {
        self.incoming[module.index()]
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
            .get(origin_module.index())?
            .get(&location)
            .map(|connection_handle| self.connections[connection_handle.index()].module)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DependencyLocation {
    block: Option<AsyncDependenciesBlockIndex>,
    dependency_index: DependencyIndex,
}
