use std::sync::Arc;

use rustc_hash::FxHashMap;

use super::{
    AsyncDependenciesBlockIndex, DependencyIndex, DependencyLocation, ModuleGraph,
    ModuleGraphStorage,
};
use crate::{
    Dependency, Error, ExportsInfo, ModuleGraphConnection, ModuleGraphConnectionHandle,
    ModuleGraphConnectionState, ModuleHandle, ModuleIdentity,
    index_vec::IndexVec,
    module::{BuildingModule, BuiltModuleContent},
};

/// Make-owned graph state. Consuming `finish` is the only way to obtain the
/// immutable `ModuleGraph` used by finishModules and sealing.
#[derive(Debug, Default, Clone)]
pub(crate) struct BuildingModuleGraph {
    storage: ModuleGraphStorage<BuildingModule>,
}

impl BuildingModuleGraph {
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
