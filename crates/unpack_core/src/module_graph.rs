// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/ModuleGraph.js

use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::{
    Dependency, Error, ExportsInfo, Module, ModuleGraphConnection, ModuleGraphConnectionHandle,
    ModuleGraphConnectionState, ModuleHandle, ModuleIdentity, module::BuildingModule,
};
use crate::{index_vec::IndexVec, module::BuiltModuleContent};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ModuleGraph {
    storage: ModuleGraphStorage<Module>,
    exports_info: IndexVec<ModuleHandle, ExportsInfo>,
}

/// Make-owned graph state. Consuming `finish` is the only way to obtain the
/// immutable `ModuleGraph` used by finishModules and sealing.
#[derive(Debug, Default, Clone)]
pub(crate) struct BuildingModuleGraph {
    storage: ModuleGraphStorage<BuildingModule>,
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

impl BuildingModuleGraph {
    pub(crate) fn module(&self, handle: ModuleHandle) -> Option<&BuildingModule> {
        self.storage.modules.get(handle.index())
    }

    pub(crate) fn add_module(
        &mut self,
        identity: ModuleIdentity,
        factory_side_effect_free: Option<bool>,
    ) -> ModuleHandle {
        let handle = ModuleHandle::new(self.storage.modules.len());
        let mut module = BuildingModule::new(handle, identity);
        module.set_factory_side_effect_free(factory_side_effect_free);
        self.storage.modules.push(module);
        let outgoing_handle = self.storage.outgoing.push(Vec::new());
        let incoming_handle = self.storage.incoming.push(Vec::new());
        let location_handle = self.storage.outgoing_by_location.push(FxHashMap::default());
        debug_assert_eq!(outgoing_handle, handle);
        debug_assert_eq!(incoming_handle, handle);
        debug_assert_eq!(location_handle, handle);
        handle
    }

    pub(crate) fn connect(
        &mut self,
        origin_module: Option<ModuleHandle>,
        origin_block: Option<AsyncDependenciesBlockIndex>,
        origin_dependency_index: Option<DependencyIndex>,
        dependency: Dependency,
        module: ModuleHandle,
    ) {
        let connection_handle = ModuleGraphConnectionHandle::new(self.storage.connections.len());
        self.storage.connections.push(ModuleGraphConnection {
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
            self.storage.outgoing[origin_module].push(connection_handle);
            if let Some(dependency_index) = origin_dependency_index {
                self.storage.outgoing_by_location[origin_module].insert(
                    DependencyLocation {
                        block: origin_block,
                        dependency_index,
                    },
                    connection_handle,
                );
            }
        }
        self.storage.incoming[module].push(connection_handle);
    }

    pub(crate) fn finish_module_build(
        &mut self,
        handle: ModuleHandle,
        content: Arc<BuiltModuleContent>,
    ) -> Result<(), Error> {
        let module = self
            .storage
            .modules
            .get_mut(handle.index())
            .ok_or(Error::MissingModule(handle))?;
        module.finish_build_content(content);
        Ok(())
    }

    pub(crate) fn fail_module(
        &mut self,
        handle: ModuleHandle,
        error: Error,
        source: String,
    ) -> Result<(), Error> {
        let module = self
            .storage
            .modules
            .get_mut(handle.index())
            .ok_or(Error::MissingModule(handle))?;
        module.fail_build(error, source);
        Ok(())
    }

    pub(crate) fn finish(self) -> ModuleGraph {
        let mut exports_info = IndexVec::default();
        for module in &self.storage.modules {
            let handle = exports_info.push(ExportsInfo::default());
            debug_assert_eq!(handle, module.handle());
        }
        let ModuleGraphStorage {
            modules,
            connections,
            outgoing,
            incoming,
            outgoing_by_location,
        } = self.storage;
        ModuleGraph {
            storage: ModuleGraphStorage {
                modules: modules.into_iter().map(BuildingModule::finish).collect(),
                connections,
                outgoing,
                incoming,
                outgoing_by_location,
            },
            exports_info,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DependencyLocation {
    block: Option<AsyncDependenciesBlockIndex>,
    dependency_index: DependencyIndex,
}

#[cfg(test)]
mod tests {
    use super::BuildingModuleGraph;
    use crate::ModuleIdentity;

    #[test]
    fn adding_a_module_initializes_its_connection_indices() {
        let mut building_module_graph = BuildingModuleGraph::default();
        let module =
            building_module_graph.add_module(ModuleIdentity::new("/project/src/index.js"), None);
        let module_graph = building_module_graph.finish();

        assert_eq!(module_graph.outgoing_connections(module).count(), 0);
        assert_eq!(module_graph.incoming_connections(module).count(), 0);
    }
}
