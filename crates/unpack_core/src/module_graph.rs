use crate::{Dependency, Module, ModuleId, ModuleIdentity};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ModuleGraph {
    modules: Vec<Module>,
    connections: Vec<ModuleGraphConnection>,
    outgoing: Vec<Vec<usize>>,
    incoming: Vec<Vec<usize>>,
}

impl ModuleGraph {
    pub(crate) fn add_module(&mut self, identity: ModuleIdentity) -> ModuleId {
        let id = ModuleId::new(self.modules.len());
        self.modules.push(Module::new(id, identity));
        self.outgoing.push(Vec::new());
        self.incoming.push(Vec::new());
        id
    }

    pub(crate) fn module_mut(&mut self, id: ModuleId) -> Option<&mut Module> {
        self.modules.get_mut(id.index())
    }

    pub(crate) fn connect(
        &mut self,
        origin_module: Option<ModuleId>,
        dependency: Dependency,
        module: ModuleId,
    ) {
        let connection_id = self.connections.len();
        self.connections.push(ModuleGraphConnection {
            origin_module,
            dependency,
            module,
        });
        if let Some(origin_module) = origin_module {
            self.outgoing[origin_module.index()].push(connection_id);
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
            .map(|connection_id| &self.connections[*connection_id])
    }

    pub fn incoming_connections(
        &self,
        module: ModuleId,
    ) -> impl Iterator<Item = &ModuleGraphConnection> {
        self.incoming[module.index()]
            .iter()
            .map(|connection_id| &self.connections[*connection_id])
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleGraphConnection {
    pub origin_module: Option<ModuleId>,
    pub dependency: Dependency,
    pub module: ModuleId,
}
