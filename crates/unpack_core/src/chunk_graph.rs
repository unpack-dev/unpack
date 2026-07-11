use std::collections::{HashMap, HashSet};

use crate::{
    ModuleId,
    chunk::{Chunk, ChunkId},
    chunk_group::{AsyncBlockOrigin, ChunkGroup, ChunkGroupId, ChunkGroupKind},
    id_assignment::RenderId,
    runtime::{
        RuntimeModule, RuntimeRequirements, entry_startup_runtime_requirements,
        resolve_runtime_modules,
    },
};

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
    pub(crate) fn add_chunk(
        &mut self,
        name: Option<String>,
        root_modules: Vec<ModuleId>,
    ) -> ChunkId {
        let id = ChunkId::new(self.chunks.len());
        self.chunks.push(Chunk::new(id, name, root_modules));
        self.chunk_modules.push(Vec::new());
        self.chunk_runtime_requirements
            .push(RuntimeRequirements::default());
        self.chunk_runtime_modules.push(Vec::new());
        id
    }

    pub(crate) fn add_chunk_group(
        &mut self,
        kind: ChunkGroupKind,
        origin: Option<AsyncBlockOrigin>,
    ) -> ChunkGroupId {
        let id = ChunkGroupId::new(self.chunk_groups.len());
        self.chunk_groups.push(ChunkGroup::new(id, kind, origin));
        self.runtime_chunk_group_children.push(Vec::new());
        id
    }

    pub(crate) fn connect_chunk_and_group(&mut self, chunk: ChunkId, group: ChunkGroupId) {
        self.chunks[chunk.index()].add_group(group);
        self.chunk_groups[group.index()].push_chunk(chunk);
    }

    pub(crate) fn connect_chunk_groups(&mut self, parent: ChunkGroupId, child: ChunkGroupId) {
        self.chunk_groups[parent.index()].add_child(child);
        self.chunk_groups[child.index()].add_parent(parent);
        self.connect_runtime_chunk_groups(parent, child);
    }

    pub(crate) fn connect_runtime_chunk_groups(
        &mut self,
        parent: ChunkGroupId,
        child: ChunkGroupId,
    ) {
        if !self.runtime_chunk_group_children[parent.index()].contains(&child) {
            self.runtime_chunk_group_children[parent.index()].push(child);
        }
    }

    pub(crate) fn add_entrypoint(&mut self, entrypoint: ChunkGroupId) {
        self.entrypoints.push(entrypoint);
    }

    pub(crate) fn connect_block_and_chunk_group(
        &mut self,
        origin: AsyncBlockOrigin,
        chunk_group: ChunkGroupId,
    ) {
        self.block_chunk_groups.insert(origin, chunk_group);
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
        self.chunks[new_chunk.index()].set_filename_override(filename.into());
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

    pub fn module_render_id_string(&self, module: ModuleId) -> Option<&str> {
        self.module_render_id(module).and_then(RenderId::as_string)
    }

    pub fn module_render_id_number(&self, module: ModuleId) -> Option<u32> {
        self.module_render_id(module).and_then(RenderId::as_number)
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
