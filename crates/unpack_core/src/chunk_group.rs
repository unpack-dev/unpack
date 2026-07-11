use crate::{AsyncDependenciesBlockIndex, ModuleHandle, chunk::ChunkHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkGroupHandle(usize);

impl ChunkGroupHandle {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkGroup {
    handle: ChunkGroupHandle,
    kind: ChunkGroupKind,
    chunks: Vec<ChunkHandle>,
    parents: Vec<ChunkGroupHandle>,
    children: Vec<ChunkGroupHandle>,
    origin: Option<AsyncBlockOrigin>,
}

impl ChunkGroup {
    pub(crate) fn new(
        handle: ChunkGroupHandle,
        kind: ChunkGroupKind,
        origin: Option<AsyncBlockOrigin>,
    ) -> Self {
        Self {
            handle,
            kind,
            chunks: Vec::new(),
            parents: Vec::new(),
            children: Vec::new(),
            origin,
        }
    }

    pub fn handle(&self) -> ChunkGroupHandle {
        self.handle
    }

    pub fn kind(&self) -> &ChunkGroupKind {
        &self.kind
    }

    pub fn chunks(&self) -> &[ChunkHandle] {
        &self.chunks
    }

    pub fn parents(&self) -> &[ChunkGroupHandle] {
        &self.parents
    }

    pub fn children(&self) -> &[ChunkGroupHandle] {
        &self.children
    }

    pub fn origin(&self) -> Option<AsyncBlockOrigin> {
        self.origin
    }

    pub(crate) fn push_chunk(&mut self, chunk: ChunkHandle) {
        if !self.chunks.contains(&chunk) {
            self.chunks.push(chunk);
        }
    }

    pub(crate) fn add_parent(&mut self, parent: ChunkGroupHandle) {
        if !self.parents.contains(&parent) {
            self.parents.push(parent);
        }
    }

    pub(crate) fn add_child(&mut self, child: ChunkGroupHandle) {
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
    pub module: ModuleHandle,
    pub block: AsyncDependenciesBlockIndex,
}
