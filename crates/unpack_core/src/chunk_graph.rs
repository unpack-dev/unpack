use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
};

use crate::{CompilerOptions, ModuleGraph, ModuleId};

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
    render_id: String,
    filename: String,
    groups: Vec<ChunkGroupId>,
}

impl Chunk {
    fn new(id: ChunkId, render_id: String, filename: String) -> Self {
        Self {
            id,
            render_id,
            filename,
            groups: Vec::new(),
        }
    }

    pub fn id(&self) -> ChunkId {
        self.id
    }

    pub fn render_id(&self) -> &str {
        &self.render_id
    }

    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn groups(&self) -> &[ChunkGroupId] {
        &self.groups
    }

    fn add_group(&mut self, group: ChunkGroupId) {
        if !self.groups.contains(&group) {
            self.groups.push(group);
        }
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
    block_chunk_groups: HashMap<AsyncBlockOrigin, ChunkGroupId>,
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
            let entry_chunk = graph.add_chunk(entry_name.clone(), format!("{entry_name}.js"));
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
                    let render_id =
                        chunk_render_id(options.context.as_path(), module_graph, target);
                    let chunk = graph.add_chunk(render_id.clone(), format!("{render_id}.js"));
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

    fn add_chunk(&mut self, render_id: String, filename: String) -> ChunkId {
        let id = ChunkId::new(self.chunks.len());
        self.chunks.push(Chunk::new(id, render_id, filename));
        self.chunk_modules.push(Vec::new());
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
        let new_chunk = self.add_chunk(render_id.into(), filename.into());
        original.split(&mut self.chunks[new_chunk.index()], &mut self.chunk_groups);
        Some(new_chunk)
    }

    pub(crate) fn connect_chunk_and_module(&mut self, chunk: ChunkId, module: ModuleId) {
        if self.module_chunks.len() <= module.index() {
            self.module_chunks.resize_with(module.index() + 1, Vec::new);
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

    pub fn block_chunk_group(&self, origin: AsyncBlockOrigin) -> Option<ChunkGroupId> {
        self.block_chunk_groups.get(&origin).copied()
    }

    pub fn chunk(&self, id: ChunkId) -> Option<&Chunk> {
        self.chunks.get(id.index())
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

fn chunk_render_id(context: &Path, module_graph: &ModuleGraph, module: ModuleId) -> String {
    let resource = module_graph
        .module(module)
        .map(|module| module.identity().resource.as_path())
        .unwrap_or_else(|| Path::new("chunk"));
    let relative = make_relative(context, resource);
    sanitize_chunk_id(&relative)
}

fn make_relative(context: &Path, resource: &Path) -> String {
    let context = std::fs::canonicalize(context).unwrap_or_else(|_| context.to_path_buf());
    let resource = std::fs::canonicalize(resource).unwrap_or_else(|_| PathBuf::from(resource));
    let relative = resource.strip_prefix(&context).unwrap_or(&resource);
    relative.to_string_lossy().replace('\\', "/")
}

fn sanitize_chunk_id(relative: &str) -> String {
    relative
        .trim_start_matches("./")
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}
