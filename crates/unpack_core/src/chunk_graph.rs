use std::collections::{HashMap, HashSet};

use crate::{
    ModuleHandle,
    chunk::{Chunk, ChunkHandle},
    chunk_group::{AsyncBlockOrigin, ChunkGroup, ChunkGroupHandle, ChunkGroupKind},
    id_assignment::{ChunkId, ModuleId},
    runtime::{
        RuntimeModule, RuntimeRequirements, entry_startup_runtime_requirements,
        resolve_runtime_modules,
    },
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ChunkGraph {
    chunks: Vec<Chunk>,
    chunk_groups: Vec<ChunkGroup>,
    entrypoints: Vec<ChunkGroupHandle>,
    chunk_modules: Vec<Vec<ModuleHandle>>,
    module_chunks: Vec<Vec<ChunkHandle>>,
    module_ids: Vec<Option<ModuleId>>,
    block_chunk_groups: HashMap<AsyncBlockOrigin, ChunkGroupHandle>,
    // Includes logical loading edges omitted from the materialized graph to break cycles.
    runtime_chunk_group_children: Vec<Vec<ChunkGroupHandle>>,
    module_runtime_requirements: Vec<RuntimeRequirements>,
    chunk_runtime_requirements: Vec<RuntimeRequirements>,
    runtime_tree_requirements: HashMap<ChunkGroupHandle, RuntimeRequirements>,
    chunk_runtime_modules: Vec<Vec<RuntimeModule>>,
}

impl ChunkGraph {
    pub(crate) fn add_chunk(
        &mut self,
        name: Option<String>,
        root_modules: Vec<ModuleHandle>,
    ) -> ChunkHandle {
        let handle = ChunkHandle::new(self.chunks.len());
        self.chunks.push(Chunk::new(handle, name, root_modules));
        self.chunk_modules.push(Vec::new());
        self.chunk_runtime_requirements
            .push(RuntimeRequirements::default());
        self.chunk_runtime_modules.push(Vec::new());
        handle
    }

    pub(crate) fn add_chunk_group(
        &mut self,
        kind: ChunkGroupKind,
        origin: Option<AsyncBlockOrigin>,
    ) -> ChunkGroupHandle {
        let handle = ChunkGroupHandle::new(self.chunk_groups.len());
        self.chunk_groups
            .push(ChunkGroup::new(handle, kind, origin));
        self.runtime_chunk_group_children.push(Vec::new());
        handle
    }

    pub(crate) fn connect_chunk_and_group(&mut self, chunk: ChunkHandle, group: ChunkGroupHandle) {
        self.chunks[chunk.index()].add_group(group);
        self.chunk_groups[group.index()].push_chunk(chunk);
    }

    pub(crate) fn connect_chunk_groups(
        &mut self,
        parent: ChunkGroupHandle,
        child: ChunkGroupHandle,
    ) {
        self.chunk_groups[parent.index()].add_child(child);
        self.chunk_groups[child.index()].add_parent(parent);
        self.connect_runtime_chunk_groups(parent, child);
    }

    pub(crate) fn connect_runtime_chunk_groups(
        &mut self,
        parent: ChunkGroupHandle,
        child: ChunkGroupHandle,
    ) {
        if !self.runtime_chunk_group_children[parent.index()].contains(&child) {
            self.runtime_chunk_group_children[parent.index()].push(child);
        }
    }

    pub(crate) fn add_entrypoint(&mut self, entrypoint: ChunkGroupHandle) {
        self.entrypoints.push(entrypoint);
    }

    pub(crate) fn connect_block_and_chunk_group(
        &mut self,
        origin: AsyncBlockOrigin,
        chunk_group: ChunkGroupHandle,
    ) {
        self.block_chunk_groups.insert(origin, chunk_group);
    }

    pub fn split_chunk(
        &mut self,
        chunk: ChunkHandle,
        id: impl Into<String>,
        filename: impl Into<String>,
    ) -> Option<ChunkHandle> {
        let original = self.chunks.get(chunk.index())?.clone();
        let id: String = id.into();
        let id = ChunkId::from(id);
        let new_chunk = self.add_chunk(None, Vec::new());
        self.chunks[new_chunk.index()].assign_id(id);
        self.chunks[new_chunk.index()].set_filename_override(filename.into());
        original.split(&mut self.chunks[new_chunk.index()], &mut self.chunk_groups);
        Some(new_chunk)
    }

    pub(crate) fn connect_chunk_and_module(&mut self, chunk: ChunkHandle, module: ModuleHandle) {
        if self.module_chunks.len() <= module.index() {
            self.module_chunks.resize_with(module.index() + 1, Vec::new);
            self.module_ids.resize_with(module.index() + 1, || None);
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

    pub fn entrypoints(&self) -> &[ChunkGroupHandle] {
        &self.entrypoints
    }

    pub fn chunk_modules(&self, chunk: ChunkHandle) -> &[ModuleHandle] {
        &self.chunk_modules[chunk.index()]
    }

    pub fn module_chunks(&self, module: ModuleHandle) -> &[ChunkHandle] {
        self.module_chunks
            .get(module.index())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn module_id(&self, module: ModuleHandle) -> Option<&ModuleId> {
        self.module_ids.get(module.index()).and_then(Option::as_ref)
    }

    pub fn module_id_string(&self, module: ModuleHandle) -> Option<&str> {
        self.module_id(module).and_then(ModuleId::as_string)
    }

    pub fn module_id_number(&self, module: ModuleHandle) -> Option<u32> {
        self.module_id(module).and_then(ModuleId::as_number)
    }

    pub(crate) fn set_module_id(&mut self, module: ModuleHandle, id: ModuleId) {
        if self.module_ids.len() <= module.index() {
            self.module_ids.resize_with(module.index() + 1, || None);
        }
        self.module_ids[module.index()] = Some(id);
    }

    pub(crate) fn set_chunk_id(&mut self, chunk: ChunkHandle, id: ChunkId) {
        self.chunks[chunk.index()].assign_id(id);
    }

    pub fn block_chunk_group(&self, origin: AsyncBlockOrigin) -> Option<ChunkGroupHandle> {
        self.block_chunk_groups.get(&origin).copied()
    }

    pub fn chunk(&self, handle: ChunkHandle) -> Option<&Chunk> {
        self.chunks.get(handle.index())
    }

    pub(crate) fn process_runtime_requirements(
        &mut self,
        module_requirements: impl IntoIterator<Item = (ModuleHandle, RuntimeRequirements)>,
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
            while let Some(group_handle) = pending.pop() {
                if !visited.insert(group_handle) {
                    continue;
                }
                let group = &self.chunk_groups[group_handle.index()];
                for chunk in group.chunks() {
                    requirements.extend(&self.chunk_runtime_requirements[chunk.index()]);
                }
                pending.extend(
                    self.runtime_chunk_group_children[group_handle.index()]
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
        module: ModuleHandle,
    ) -> Option<&RuntimeRequirements> {
        self.module_runtime_requirements.get(module.index())
    }

    #[cfg(test)]
    pub(crate) fn chunk_runtime_requirements(
        &self,
        chunk: ChunkHandle,
    ) -> Option<&RuntimeRequirements> {
        self.chunk_runtime_requirements.get(chunk.index())
    }

    pub(crate) fn runtime_tree_requirements(
        &self,
        entrypoint: ChunkGroupHandle,
    ) -> Option<&RuntimeRequirements> {
        self.runtime_tree_requirements.get(&entrypoint)
    }

    pub(crate) fn runtime_modules(&self, chunk: ChunkHandle) -> &[RuntimeModule] {
        self.chunk_runtime_modules
            .get(chunk.index())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}
