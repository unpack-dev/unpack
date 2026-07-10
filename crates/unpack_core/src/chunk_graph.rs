use std::collections::{HashMap, HashSet, VecDeque};

use crate::{
    CompilerOptions, ModuleGraph, ModuleId,
    id_assignment::RenderId,
    runtime::{
        RuntimeModule, RuntimeRequirements, entry_startup_runtime_requirements,
        resolve_runtime_modules,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkId(usize);

impl ChunkId {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkGroupId(usize);

impl ChunkGroupId {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    id: ChunkId,
    name: Option<String>,
    root_modules: Vec<ModuleId>,
    render_id: Option<RenderId>,
    filename_override: Option<String>,
    groups: Vec<ChunkGroupId>,
}

impl Chunk {
    fn new(id: ChunkId, name: Option<String>, root_modules: Vec<ModuleId>) -> Self {
        Self {
            id,
            name,
            root_modules,
            render_id: None,
            filename_override: None,
            groups: Vec::new(),
        }
    }

    pub fn id(&self) -> ChunkId {
        self.id
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub(crate) fn render_id(&self) -> &RenderId {
        self.render_id
            .as_ref()
            .expect("chunk Render ID should be assigned before it is read")
    }

    pub fn groups(&self) -> &[ChunkGroupId] {
        &self.groups
    }

    fn add_group(&mut self, group: ChunkGroupId) {
        if !self.groups.contains(&group) {
            self.groups.push(group);
        }
    }

    pub(crate) fn root_modules(&self) -> &[ModuleId] {
        &self.root_modules
    }

    pub(crate) fn filename_override(&self) -> Option<&str> {
        self.filename_override.as_deref()
    }

    fn assign_render_id(&mut self, render_id: RenderId) {
        self.render_id = Some(render_id);
    }

    pub fn split(&self, new_chunk: &mut Chunk, chunk_groups: &mut [ChunkGroup]) {
        for group in &self.groups {
            new_chunk.add_group(*group);
            chunk_groups[group.index()].push_chunk(new_chunk.id());
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkGroup {
    id: ChunkGroupId,
    kind: ChunkGroupKind,
    chunks: Vec<ChunkId>,
    parents: Vec<ChunkGroupId>,
    children: Vec<ChunkGroupId>,
    origin: Option<AsyncBlockOrigin>,
}

impl ChunkGroup {
    fn new(id: ChunkGroupId, kind: ChunkGroupKind, origin: Option<AsyncBlockOrigin>) -> Self {
        Self {
            id,
            kind,
            chunks: Vec::new(),
            parents: Vec::new(),
            children: Vec::new(),
            origin,
        }
    }

    pub fn id(&self) -> ChunkGroupId {
        self.id
    }

    pub fn kind(&self) -> &ChunkGroupKind {
        &self.kind
    }

    pub fn chunks(&self) -> &[ChunkId] {
        &self.chunks
    }

    pub fn parents(&self) -> &[ChunkGroupId] {
        &self.parents
    }

    pub fn children(&self) -> &[ChunkGroupId] {
        &self.children
    }

    pub fn origin(&self) -> Option<AsyncBlockOrigin> {
        self.origin
    }

    fn push_chunk(&mut self, chunk: ChunkId) {
        if !self.chunks.contains(&chunk) {
            self.chunks.push(chunk);
        }
    }

    fn add_parent(&mut self, parent: ChunkGroupId) {
        if !self.parents.contains(&parent) {
            self.parents.push(parent);
        }
    }

    fn add_child(&mut self, child: ChunkGroupId) {
        if !self.children.contains(&child) {
            self.children.push(child);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkGroupKind {
    Entrypoint { name: String },
    Async,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AsyncBlockOrigin {
    pub module: ModuleId,
    pub block_index: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ChunkGraph {
    chunks: Vec<Chunk>,
    chunk_groups: Vec<ChunkGroup>,
    entrypoints: Vec<ChunkGroupId>,
    chunk_modules: Vec<Vec<ModuleId>>,
    module_chunks: Vec<Vec<ChunkId>>,
    module_render_ids: Vec<Option<RenderId>>,
    block_chunk_groups: HashMap<AsyncBlockOrigin, ChunkGroupId>,
    module_runtime_requirements: Vec<RuntimeRequirements>,
    chunk_runtime_requirements: Vec<RuntimeRequirements>,
    runtime_tree_requirements: HashMap<ChunkGroupId, RuntimeRequirements>,
    chunk_runtime_modules: Vec<Vec<RuntimeModule>>,
}

impl ChunkGraph {
    pub(crate) fn build(
        options: &CompilerOptions,
        module_graph: &ModuleGraph,
        entries: &[ModuleId],
    ) -> Self {
        let mut graph = Self::default();
        let mut async_groups_by_target = HashMap::new();
        for (entry_index, entry_module) in entries.iter().copied().enumerate() {
            let entry_name = options
                .entries
                .get(entry_index)
                .map(|entry| entry.name.clone())
                .unwrap_or_else(|| format!("entry{entry_index}"));
            let entry_chunk = graph.add_chunk(Some(entry_name.clone()), vec![entry_module]);
            let entry_group = graph.add_chunk_group(
                ChunkGroupKind::Entrypoint {
                    name: entry_name.clone(),
                },
                None,
            );
            graph.connect_chunk_and_group(entry_chunk, entry_group);
            graph.entrypoints.push(entry_group);

            let initial_modules = collect_static_reachable(module_graph, entry_module, None);
            for module in &initial_modules {
                graph.connect_chunk_and_module(entry_chunk, *module);
            }

            let initial_set = initial_modules.iter().copied().collect::<HashSet<_>>();
            let mut async_origins = Vec::new();
            for module in &initial_modules {
                if let Some(module_ref) = module_graph.module(*module) {
                    for (block_index, block) in module_ref.blocks().iter().enumerate() {
                        if block
                            .dependencies()
                            .iter()
                            .any(|dep| dep.is_import_dependency())
                        {
                            async_origins.push(AsyncBlockOrigin {
                                module: *module,
                                block_index,
                            });
                        }
                    }
                }
            }

            for origin in async_origins {
                let Some(target) = import_block_target(module_graph, origin) else {
                    continue;
                };
                let async_modules =
                    collect_static_reachable(module_graph, target, Some(&initial_set));
                if async_modules.is_empty() {
                    continue;
                }

                let async_group = if let Some(group) = async_groups_by_target.get(&target).copied()
                {
                    group
                } else {
                    let chunk = graph.add_chunk(None, vec![target]);
                    let group = graph.add_chunk_group(ChunkGroupKind::Async, Some(origin));
                    graph.connect_chunk_and_group(chunk, group);
                    async_groups_by_target.insert(target, group);
                    group
                };

                if let Some(chunk) = graph.chunk_groups()[async_group.index()]
                    .chunks()
                    .first()
                    .copied()
                {
                    for module in async_modules {
                        graph.connect_chunk_and_module(chunk, module);
                    }
                }

                graph.block_chunk_groups.insert(origin, async_group);
                graph.connect_chunk_groups(entry_group, async_group);
            }
        }

        graph
    }

    fn add_chunk(&mut self, name: Option<String>, root_modules: Vec<ModuleId>) -> ChunkId {
        let id = ChunkId::new(self.chunks.len());
        self.chunks.push(Chunk::new(id, name, root_modules));
        self.chunk_modules.push(Vec::new());
        self.chunk_runtime_requirements
            .push(RuntimeRequirements::default());
        self.chunk_runtime_modules.push(Vec::new());
        id
    }

    fn add_chunk_group(
        &mut self,
        kind: ChunkGroupKind,
        origin: Option<AsyncBlockOrigin>,
    ) -> ChunkGroupId {
        let id = ChunkGroupId::new(self.chunk_groups.len());
        self.chunk_groups.push(ChunkGroup::new(id, kind, origin));
        id
    }

    fn connect_chunk_and_group(&mut self, chunk: ChunkId, group: ChunkGroupId) {
        self.chunks[chunk.index()].add_group(group);
        self.chunk_groups[group.index()].push_chunk(chunk);
    }

    fn connect_chunk_groups(&mut self, parent: ChunkGroupId, child: ChunkGroupId) {
        self.chunk_groups[parent.index()].add_child(child);
        self.chunk_groups[child.index()].add_parent(parent);
    }

    pub fn split_chunk(
        &mut self,
        chunk: ChunkId,
        render_id: impl Into<String>,
        filename: impl Into<String>,
    ) -> Option<ChunkId> {
        let original = self.chunks.get(chunk.index())?.clone();
        let render_id = RenderId::String(render_id.into());
        let new_chunk = self.add_chunk(None, Vec::new());
        self.chunks[new_chunk.index()].assign_render_id(render_id);
        self.chunks[new_chunk.index()].filename_override = Some(filename.into());
        original.split(&mut self.chunks[new_chunk.index()], &mut self.chunk_groups);
        Some(new_chunk)
    }

    pub(crate) fn connect_chunk_and_module(&mut self, chunk: ChunkId, module: ModuleId) {
        if self.module_chunks.len() <= module.index() {
            self.module_chunks.resize_with(module.index() + 1, Vec::new);
            self.module_render_ids
                .resize_with(module.index() + 1, || None);
            self.module_runtime_requirements
                .resize_with(module.index() + 1, RuntimeRequirements::default);
        }
        if !self.chunk_modules[chunk.index()].contains(&module) {
            self.chunk_modules[chunk.index()].push(module);
        }
        if !self.module_chunks[module.index()].contains(&chunk) {
            self.module_chunks[module.index()].push(chunk);
        }
    }

    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    pub fn chunk_groups(&self) -> &[ChunkGroup] {
        &self.chunk_groups
    }

    pub fn entrypoints(&self) -> &[ChunkGroupId] {
        &self.entrypoints
    }

    pub fn chunk_modules(&self, chunk: ChunkId) -> &[ModuleId] {
        &self.chunk_modules[chunk.index()]
    }

    pub fn module_chunks(&self, module: ModuleId) -> &[ChunkId] {
        self.module_chunks
            .get(module.index())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(crate) fn module_render_id(&self, module: ModuleId) -> Option<&RenderId> {
        self.module_render_ids
            .get(module.index())
            .and_then(Option::as_ref)
    }

    pub(crate) fn set_module_render_id(&mut self, module: ModuleId, render_id: RenderId) {
        if self.module_render_ids.len() <= module.index() {
            self.module_render_ids
                .resize_with(module.index() + 1, || None);
        }
        self.module_render_ids[module.index()] = Some(render_id);
    }

    pub(crate) fn set_chunk_render_id(&mut self, chunk: ChunkId, render_id: RenderId) {
        self.chunks[chunk.index()].assign_render_id(render_id);
    }

    pub fn block_chunk_group(&self, origin: AsyncBlockOrigin) -> Option<ChunkGroupId> {
        self.block_chunk_groups.get(&origin).copied()
    }

    pub fn chunk(&self, id: ChunkId) -> Option<&Chunk> {
        self.chunks.get(id.index())
    }

    pub(crate) fn process_runtime_requirements(
        &mut self,
        module_requirements: impl IntoIterator<Item = (ModuleId, RuntimeRequirements)>,
    ) {
        self.module_runtime_requirements
            .resize_with(self.module_chunks.len(), RuntimeRequirements::default);
        for requirements in &mut self.module_runtime_requirements {
            *requirements = RuntimeRequirements::default();
        }
        for (module, direct) in module_requirements {
            assert!(
                module.index() < self.module_runtime_requirements.len(),
                "Runtime Requirements must reference a Module in the Chunk Graph"
            );
            let (processed, _) = resolve_runtime_modules(&direct);
            self.module_runtime_requirements[module.index()] = processed;
        }

        for chunk_index in 0..self.chunks.len() {
            let mut requirements = RuntimeRequirements::default();
            for module in &self.chunk_modules[chunk_index] {
                requirements.extend(&self.module_runtime_requirements[module.index()]);
            }
            self.chunk_runtime_requirements[chunk_index] = requirements;
            self.chunk_runtime_modules[chunk_index].clear();
        }

        self.runtime_tree_requirements.clear();
        for entrypoint in self.entrypoints.iter().copied() {
            let mut requirements = RuntimeRequirements::default();
            let mut visited = HashSet::new();
            let mut pending = vec![entrypoint];
            while let Some(group_id) = pending.pop() {
                if !visited.insert(group_id) {
                    continue;
                }
                let group = &self.chunk_groups[group_id.index()];
                for chunk in group.chunks() {
                    requirements.extend(&self.chunk_runtime_requirements[chunk.index()]);
                }
                pending.extend(group.children().iter().copied());
            }
            requirements.extend(&entry_startup_runtime_requirements());
            let (processed, modules) = resolve_runtime_modules(&requirements);
            let runtime_chunk = self.chunk_groups[entrypoint.index()]
                .chunks()
                .first()
                .copied()
                .expect("Entrypoint must contain a runtime Chunk");
            self.chunk_runtime_modules[runtime_chunk.index()] = modules;
            self.runtime_tree_requirements.insert(entrypoint, processed);
        }
    }

    #[cfg(test)]
    pub(crate) fn module_runtime_requirements(
        &self,
        module: ModuleId,
    ) -> Option<&RuntimeRequirements> {
        self.module_runtime_requirements.get(module.index())
    }

    #[cfg(test)]
    pub(crate) fn chunk_runtime_requirements(
        &self,
        chunk: ChunkId,
    ) -> Option<&RuntimeRequirements> {
        self.chunk_runtime_requirements.get(chunk.index())
    }

    pub(crate) fn runtime_tree_requirements(
        &self,
        entrypoint: ChunkGroupId,
    ) -> Option<&RuntimeRequirements> {
        self.runtime_tree_requirements.get(&entrypoint)
    }

    pub(crate) fn runtime_modules(&self, chunk: ChunkId) -> &[RuntimeModule] {
        self.chunk_runtime_modules
            .get(chunk.index())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

fn collect_static_reachable(
    module_graph: &ModuleGraph,
    start: ModuleId,
    excluded: Option<&HashSet<ModuleId>>,
) -> Vec<ModuleId> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::from([start]);
    let mut modules = Vec::new();

    while let Some(module) = queue.pop_front() {
        if excluded.is_some_and(|excluded| excluded.contains(&module)) {
            continue;
        }
        if !visited.insert(module) {
            continue;
        }
        modules.push(module);

        for connection in module_graph.outgoing_connections(module) {
            if connection.origin_block.is_none()
                && connection.dependency.is_static_module_dependency()
            {
                queue.push_back(connection.module);
            }
        }
    }

    modules
}

fn import_block_target(module_graph: &ModuleGraph, origin: AsyncBlockOrigin) -> Option<ModuleId> {
    module_graph
        .outgoing_connections(origin.module)
        .find(|connection| {
            connection.origin_block == Some(origin.block_index)
                && connection.dependency.is_import_dependency()
        })
        .map(|connection| connection.module)
}
