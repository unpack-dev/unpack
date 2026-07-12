use crate::{
    ModuleHandle,
    chunk_group::{ChunkGroup, ChunkGroupHandle},
    id_assignment::ChunkId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkHandle(usize);

impl ChunkHandle {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    handle: ChunkHandle,
    name: Option<String>,
    root_modules: Vec<ModuleHandle>,
    id: Option<ChunkId>,
    filename_override: Option<String>,
    groups: Vec<ChunkGroupHandle>,
}

impl Chunk {
    pub(crate) fn new(
        handle: ChunkHandle,
        name: Option<String>,
        root_modules: Vec<ModuleHandle>,
    ) -> Self {
        Self {
            handle,
            name,
            root_modules,
            id: None,
            filename_override: None,
            groups: Vec::new(),
        }
    }

    pub fn handle(&self) -> ChunkHandle {
        self.handle
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn id(&self) -> Option<&ChunkId> {
        self.id.as_ref()
    }

    pub fn id_string(&self) -> Option<&str> {
        self.id().and_then(ChunkId::as_string)
    }

    pub fn id_number(&self) -> Option<u32> {
        self.id().and_then(ChunkId::as_number)
    }

    pub(crate) fn expect_id(&self) -> &ChunkId {
        self.id
            .as_ref()
            .expect("chunk ID should be assigned before it is read")
    }

    pub fn groups(&self) -> &[ChunkGroupHandle] {
        &self.groups
    }

    pub(crate) fn add_group(&mut self, group: ChunkGroupHandle) {
        if !self.groups.contains(&group) {
            self.groups.push(group);
        }
    }

    pub(crate) fn root_modules(&self) -> &[ModuleHandle] {
        &self.root_modules
    }

    pub(crate) fn filename_override(&self) -> Option<&str> {
        self.filename_override.as_deref()
    }

    pub(crate) fn set_filename_override(&mut self, filename: String) {
        self.filename_override = Some(filename);
    }

    pub(crate) fn assign_id(&mut self, id: ChunkId) {
        self.id = Some(id);
    }

    pub fn split(&self, new_chunk: &mut Chunk, chunk_groups: &mut [ChunkGroup]) {
        for group in &self.groups {
            new_chunk.add_group(*group);
            chunk_groups[group.index()].push_chunk(new_chunk.handle());
        }
    }
}
