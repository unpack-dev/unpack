// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/Chunk.js

use crate::{
    ModuleHandle,
    chunk_group::{ChunkGroup, ChunkGroupHandle},
    id_assignment::RenderId,
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
    render_id: Option<RenderId>,
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
            render_id: None,
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

    pub fn render_id_string(&self) -> Option<&str> {
        self.render_id.as_ref().and_then(RenderId::as_string)
    }

    pub fn render_id_number(&self) -> Option<u32> {
        self.render_id.as_ref().and_then(RenderId::as_number)
    }

    pub(crate) fn render_id(&self) -> &RenderId {
        self.render_id
            .as_ref()
            .expect("chunk Render ID should be assigned before it is read")
    }

    pub fn groups(&self) -> &[ChunkGroupHandle] {
        &self.groups
    }

    pub(crate) fn add_group(&mut self, group: ChunkGroupHandle) {
        if !self.groups.contains(&group) {
            self.groups.push(group);
        }
    }

    pub fn root_modules(&self) -> &[ModuleHandle] {
        &self.root_modules
    }

    pub fn filename(&self) -> String {
        crate::output_filename::resolve_chunk_filename(self)
    }

    pub(crate) fn filename_override(&self) -> Option<&str> {
        self.filename_override.as_deref()
    }

    pub(crate) fn set_filename_override(&mut self, filename: String) {
        self.filename_override = Some(filename);
    }

    pub(crate) fn assign_render_id(&mut self, render_id: RenderId) {
        self.render_id = Some(render_id);
    }

    pub fn split(&self, new_chunk: &mut Chunk, chunk_groups: &mut [ChunkGroup]) {
        for group in &self.groups {
            new_chunk.add_group(*group);
            chunk_groups[group.index()].push_chunk(new_chunk.handle());
        }
    }
}
