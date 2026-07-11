use std::collections::HashMap;

use crate::{Dependency, Module, ModuleId, ModuleIdentity};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ModuleGraph {
    modules: Vec<Module>,
    connections: Vec<ModuleGraphConnection>,
    outgoing: Vec<Vec<ModuleGraphConnectionId>>,
    incoming: Vec<Vec<ModuleGraphConnectionId>>,
    outgoing_by_location: Vec<HashMap<DependencyLocation, ModuleGraphConnectionId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct ModuleGraphConnectionId(usize);

impl ModuleGraphConnectionId {
    const fn new(index: usize) -> Self {
        Self(index)
    }

    const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DependencyId(usize);

impl DependencyId {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AsyncDependenciesBlockId(usize);

impl AsyncDependenciesBlockId {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

impl ModuleGraph {
    pub(crate) fn add_module(&mut self, identity: ModuleIdentity) -> ModuleId {
        let id = ModuleId::new(self.modules.len());
        self.modules.push(Module::new(id, identity));
        self.outgoing.push(Vec::new());
        self.incoming.push(Vec::new());
        self.outgoing_by_location.push(HashMap::new());
        id
    }

    pub(crate) fn module_mut(&mut self, id: ModuleId) -> Option<&mut Module> {
        self.modules.get_mut(id.index())
    }

    pub(crate) fn connect(
        &mut self,
        origin_module: Option<ModuleId>,
        origin_block: Option<AsyncDependenciesBlockId>,
        origin_dependency_id: Option<DependencyId>,
        dependency: Dependency,
        module: ModuleId,
    ) {
        let connection_id = ModuleGraphConnectionId::new(self.connections.len());
        self.connections.push(ModuleGraphConnection {
            origin_module,
            origin_block,
            origin_dependency_id,
            dependency,
            module,
        });
        if let Some(origin_module) = origin_module {
            self.outgoing[origin_module.index()].push(connection_id);
            if let Some(dependency_id) = origin_dependency_id {
                self.outgoing_by_location[origin_module.index()].insert(
                    DependencyLocation {
                        block: origin_block,
                        dependency_id,
                    },
                    connection_id,
                );
            }
        }
        self.incoming[module.index()].push(connection_id);
    }

    pub fn modules(&self) -> &[Module] {
        &self.modules
    }

    pub fn module(&self, id: ModuleId) -> Option<&Module> {
        self.modules.get(id.index())
    }

    pub fn connections(&self) -> &[ModuleGraphConnection] {
        &self.connections
    }

    pub fn outgoing_connections(
        &self,
        module: ModuleId,
    ) -> impl Iterator<Item = &ModuleGraphConnection> {
        self.outgoing[module.index()]
            .iter()
            .map(|connection_id| &self.connections[connection_id.index()])
    }

    pub fn incoming_connections(
        &self,
        module: ModuleId,
    ) -> impl Iterator<Item = &ModuleGraphConnection> {
        self.incoming[module.index()]
            .iter()
            .map(|connection_id| &self.connections[connection_id.index()])
    }

    pub fn module_for_dependency(
        &self,
        origin_module: ModuleId,
        origin_block: Option<AsyncDependenciesBlockId>,
        dependency_id: DependencyId,
    ) -> Option<ModuleId> {
        let location = DependencyLocation {
            block: origin_block,
            dependency_id,
        };
        self.outgoing_by_location
            .get(origin_module.index())?
            .get(&location)
            .map(|connection_id| self.connections[connection_id.index()].module)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DependencyLocation {
    block: Option<AsyncDependenciesBlockId>,
    dependency_id: DependencyId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleGraphConnection {
    pub origin_module: Option<ModuleId>,
    pub origin_block: Option<AsyncDependenciesBlockId>,
    pub origin_dependency_id: Option<DependencyId>,
    pub dependency: Dependency,
    pub module: ModuleId,
}
