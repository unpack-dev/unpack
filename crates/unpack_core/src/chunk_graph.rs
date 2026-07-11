use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet, VecDeque},
};

use crate::{
    AsyncDependenciesBlockId, CompilerOptions, ModuleGraph, ModuleId,
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
    pub block: AsyncDependenciesBlockId,
}

struct EntrypointAsyncPlan {
    group: ChunkGroupId,
    available_modules: HashSet<ModuleId>,
    modules: Vec<ModuleId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LogicalChunkGroup {
    Entrypoint(usize),
    Async(ModuleId),
}

struct AsyncParentPlan {
    available_modules: HashSet<ModuleId>,
    origins: Vec<AsyncBlockOrigin>,
}

struct AsyncChunkPlan {
    target: ModuleId,
    static_modules: Vec<ModuleId>,
    available_before: HashSet<ModuleId>,
    available_after: HashSet<ModuleId>,
    parents: HashMap<LogicalChunkGroup, AsyncParentPlan>,
}

impl AsyncChunkPlan {
    fn new(
        target: ModuleId,
        static_modules: Vec<ModuleId>,
        parent: LogicalChunkGroup,
        parent_available: &HashSet<ModuleId>,
        origin: AsyncBlockOrigin,
    ) -> Self {
        let available_before = parent_available.clone();
        let mut available_after = available_before.clone();
        available_after.extend(static_modules.iter().copied());
        Self {
            target,
            static_modules,
            available_before,
            available_after,
            parents: HashMap::from([(
                parent,
                AsyncParentPlan {
                    available_modules: parent_available.clone(),
                    origins: vec![origin],
                },
            )]),
        }
    }

    fn add_parent(
        &mut self,
        parent: LogicalChunkGroup,
        parent_available: &HashSet<ModuleId>,
        origin: AsyncBlockOrigin,
    ) -> bool {
        let parent_plan = self
            .parents
            .entry(parent)
            .or_insert_with(|| AsyncParentPlan {
                available_modules: parent_available.clone(),
                origins: Vec::new(),
            });
        parent_plan.available_modules = parent_available.clone();
        if !parent_plan.origins.contains(&origin) {
            parent_plan.origins.push(origin);
        }

        let mut parents = self.parents.values();
        let mut available_before = parents
            .next()
            .map(|parent| parent.available_modules.clone())
            .expect("Async Chunk Plan must retain at least one parent");
        for parent in parents {
            available_before.retain(|module| parent.available_modules.contains(module));
        }
        let mut available_after = available_before.clone();
        available_after.extend(self.static_modules.iter().copied());
        let changed =
            self.available_before != available_before || self.available_after != available_after;
        self.available_before = available_before;
        self.available_after = available_after;
        changed
    }
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
    // Includes logical loading edges omitted from the materialized graph to break cycles.
    runtime_chunk_group_children: Vec<Vec<ChunkGroupId>>,
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
        let mut entrypoint_plans = Vec::new();
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

            entrypoint_plans.push(EntrypointAsyncPlan {
                group: entry_group,
                available_modules: initial_modules.iter().copied().collect(),
                modules: initial_modules,
            });
        }

        let mut async_plans = HashMap::<ModuleId, AsyncChunkPlan>::new();
        let mut pending = (0..entrypoint_plans.len())
            .map(LogicalChunkGroup::Entrypoint)
            .collect::<VecDeque<_>>();
        while let Some(parent) = pending.pop_front() {
            let (parent_modules, parent_available) = match parent {
                LogicalChunkGroup::Entrypoint(index) => (
                    entrypoint_plans[index].modules.clone(),
                    entrypoint_plans[index].available_modules.clone(),
                ),
                LogicalChunkGroup::Async(target) => {
                    let plan = async_plans
                        .get(&target)
                        .expect("pending Async Chunk Plan must exist");
                    (
                        plan.static_modules
                            .iter()
                            .filter(|module| !plan.available_before.contains(module))
                            .copied()
                            .collect(),
                        plan.available_after.clone(),
                    )
                }
            };

            for (origin, target) in
                dynamic_import_origins(module_graph, parent_modules.iter().copied())
            {
                if parent_available.contains(&target) {
                    continue;
                }
                if let Some(plan) = async_plans.get_mut(&target) {
                    if plan.add_parent(parent, &parent_available, origin) {
                        pending.push_back(LogicalChunkGroup::Async(target));
                    }
                } else {
                    let static_modules = collect_static_reachable(module_graph, target, None);
                    async_plans.insert(
                        target,
                        AsyncChunkPlan::new(
                            target,
                            static_modules,
                            parent,
                            &parent_available,
                            origin,
                        ),
                    );
                    pending.push_back(LogicalChunkGroup::Async(target));
                }
            }
        }

        let mut ordered_targets = async_plans.keys().copied().collect::<Vec<_>>();
        ordered_targets
            .sort_by(|left, right| compare_module_identities(module_graph, *left, *right));

        let mut materialized_groups = HashMap::new();
        for target in &ordered_targets {
            let plan = &async_plans[target];
            let origin = plan
                .parents
                .values()
                .flat_map(|parent| parent.origins.iter().copied())
                .min_by(|left, right| compare_async_origins(module_graph, *left, *right))
                .expect("Async Chunk Plan must have at least one parent origin");
            let chunk = graph.add_chunk(None, vec![plan.target]);
            let group = graph.add_chunk_group(ChunkGroupKind::Async, Some(origin));
            graph.connect_chunk_and_group(chunk, group);
            for module in plan
                .static_modules
                .iter()
                .filter(|module| !plan.available_before.contains(module))
            {
                graph.connect_chunk_and_module(chunk, *module);
            }
            materialized_groups.insert(*target, group);
        }

        for target in ordered_targets {
            let child_group = materialized_groups[&target];
            let plan = &async_plans[&target];
            let mut parents = plan.parents.iter().collect::<Vec<_>>();
            parents.sort_by(|(left, _), (right, _)| {
                compare_logical_groups(module_graph, **left, **right)
            });
            for (parent, parent_plan) in parents {
                let parent_group = match parent {
                    LogicalChunkGroup::Entrypoint(index) => entrypoint_plans[*index].group,
                    LogicalChunkGroup::Async(target) => materialized_groups[target],
                };
                if !chunk_group_reaches(&graph, child_group, parent_group) {
                    graph.connect_chunk_groups(parent_group, child_group);
                } else {
                    graph.connect_runtime_chunk_groups(parent_group, child_group);
                }
                for origin in &parent_plan.origins {
                    graph.block_chunk_groups.insert(*origin, child_group);
                }
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
        self.runtime_chunk_group_children.push(Vec::new());
        id
    }

    fn connect_chunk_and_group(&mut self, chunk: ChunkId, group: ChunkGroupId) {
        self.chunks[chunk.index()].add_group(group);
        self.chunk_groups[group.index()].push_chunk(chunk);
    }

    fn connect_chunk_groups(&mut self, parent: ChunkGroupId, child: ChunkGroupId) {
        self.chunk_groups[parent.index()].add_child(child);
        self.chunk_groups[child.index()].add_parent(parent);
        self.connect_runtime_chunk_groups(parent, child);
    }

    fn connect_runtime_chunk_groups(&mut self, parent: ChunkGroupId, child: ChunkGroupId) {
        if !self.runtime_chunk_group_children[parent.index()].contains(&child) {
            self.runtime_chunk_group_children[parent.index()].push(child);
        }
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
                pending.extend(
                    self.runtime_chunk_group_children[group_id.index()]
                        .iter()
                        .copied(),
                );
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

fn dynamic_import_origins(
    module_graph: &ModuleGraph,
    modules: impl IntoIterator<Item = ModuleId>,
) -> Vec<(AsyncBlockOrigin, ModuleId)> {
    let mut origins = Vec::new();
    for module in modules {
        let module_ref = module_graph
            .module(module)
            .expect("Chunk planning must reference an existing Module");
        for (block_index, block) in module_ref.blocks().iter().enumerate() {
            if !block
                .dependencies()
                .iter()
                .any(|dependency| dependency.is_import_dependency())
            {
                continue;
            }
            let origin = AsyncBlockOrigin {
                module,
                block: AsyncDependenciesBlockId::new(block_index),
            };
            if let Some(target) = import_block_target(module_graph, origin) {
                origins.push((origin, target));
            }
        }
    }
    origins.sort_by(|(left_origin, left_target), (right_origin, right_target)| {
        compare_async_origins(module_graph, *left_origin, *right_origin)
            .then_with(|| compare_module_identities(module_graph, *left_target, *right_target))
    });
    origins.dedup();
    origins
}

fn compare_async_origins(
    module_graph: &ModuleGraph,
    left: AsyncBlockOrigin,
    right: AsyncBlockOrigin,
) -> Ordering {
    compare_module_identities(module_graph, left.module, right.module)
        .then(left.block.cmp(&right.block))
}

fn compare_logical_groups(
    module_graph: &ModuleGraph,
    left: LogicalChunkGroup,
    right: LogicalChunkGroup,
) -> Ordering {
    match (left, right) {
        (LogicalChunkGroup::Entrypoint(left), LogicalChunkGroup::Entrypoint(right)) => {
            left.cmp(&right)
        }
        (LogicalChunkGroup::Entrypoint(_), LogicalChunkGroup::Async(_)) => Ordering::Less,
        (LogicalChunkGroup::Async(_), LogicalChunkGroup::Entrypoint(_)) => Ordering::Greater,
        (LogicalChunkGroup::Async(left), LogicalChunkGroup::Async(right)) => {
            compare_module_identities(module_graph, left, right)
        }
    }
}

fn compare_module_identities(
    module_graph: &ModuleGraph,
    left: ModuleId,
    right: ModuleId,
) -> Ordering {
    let left_identity = module_graph
        .module(left)
        .expect("Chunk planning order must reference an existing Module")
        .identity();
    let right_identity = module_graph
        .module(right)
        .expect("Chunk planning order must reference an existing Module")
        .identity();
    left_identity.cmp(right_identity)
}

fn chunk_group_reaches(graph: &ChunkGraph, start: ChunkGroupId, target: ChunkGroupId) -> bool {
    let mut visited = HashSet::new();
    let mut pending = VecDeque::from([start]);
    while let Some(group) = pending.pop_front() {
        if group == target {
            return true;
        }
        if !visited.insert(group) {
            continue;
        }
        pending.extend(
            graph.chunk_groups()[group.index()]
                .children()
                .iter()
                .copied(),
        );
    }
    false
}

fn import_block_target(module_graph: &ModuleGraph, origin: AsyncBlockOrigin) -> Option<ModuleId> {
    module_graph
        .outgoing_connections(origin.module)
        .find(|connection| {
            connection.origin_block == Some(origin.block)
                && connection.dependency.is_import_dependency()
        })
        .map(|connection| connection.module)
}
