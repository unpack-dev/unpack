use crate::{AsyncDependenciesBlockId, ModuleId, chunk::ChunkId};

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
pub struct ChunkGroup {
    id: ChunkGroupId,
    kind: ChunkGroupKind,
    chunks: Vec<ChunkId>,
    parents: Vec<ChunkGroupId>,
    children: Vec<ChunkGroupId>,
    origin: Option<AsyncBlockOrigin>,
}

impl ChunkGroup {
    pub(crate) fn new(
        id: ChunkGroupId,
        kind: ChunkGroupKind,
        origin: Option<AsyncBlockOrigin>,
    ) -> Self {
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

    pub(crate) fn push_chunk(&mut self, chunk: ChunkId) {
        if !self.chunks.contains(&chunk) {
            self.chunks.push(chunk);
        }
    }

    pub(crate) fn add_parent(&mut self, parent: ChunkGroupId) {
        if !self.parents.contains(&parent) {
            self.parents.push(parent);
        }
    }

    pub(crate) fn add_child(&mut self, child: ChunkGroupId) {
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
