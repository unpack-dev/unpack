// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/ChunkGraph.js

use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    ModuleGraph, ModuleHandle,
    chunk::{Chunk, ChunkHandle},
    chunk_group::{AsyncBlockOrigin, ChunkGroup, ChunkGroupHandle, ChunkGroupKind},
    id_assignment::RenderId,
    runtime::{
        RuntimeModule, RuntimeRequirements, entry_startup_runtime_requirements,
        resolve_runtime_modules,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ModuleHash(u64);

impl ModuleHash {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ChunkGraphModuleReferences {
    pub(crate) module_render_id: Option<RenderId>,
    pub(crate) chunk_render_ids: Vec<RenderId>,
    pub(crate) outgoing_module_render_ids: Vec<Option<RenderId>>,
    pub(crate) block_chunk_render_ids: Vec<Option<Vec<RenderId>>>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ChunkGraph {
    chunks: Vec<Chunk>,
    chunk_groups: Vec<ChunkGroup>,
    entrypoints: Vec<ChunkGroupHandle>,
    chunk_modules: Vec<Vec<ModuleHandle>>,
    module_chunks: Vec<Vec<ChunkHandle>>,
    module_render_ids: Vec<Option<RenderId>>,
    module_hashes: Vec<Option<ModuleHash>>,
    block_chunk_groups: FxHashMap<AsyncBlockOrigin, ChunkGroupHandle>,
    // Includes logical loading edges omitted from the materialized graph to break cycles.
    runtime_chunk_group_children: Vec<Vec<ChunkGroupHandle>>,
    module_runtime_requirements: Vec<RuntimeRequirements>,
    chunk_runtime_requirements: Vec<RuntimeRequirements>,
    runtime_tree_requirements: FxHashMap<ChunkGroupHandle, RuntimeRequirements>,
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
        render_id: impl Into<String>,
        filename: impl Into<String>,
    ) -> Option<ChunkHandle> {
        let original = self.chunks.get(chunk.index())?.clone();
        let render_id = RenderId::String(render_id.into());
        let new_chunk = self.add_chunk(None, Vec::new());
        self.chunks[new_chunk.index()].assign_render_id(render_id);
        self.chunks[new_chunk.index()].set_filename_override(filename.into());
        original.split(&mut self.chunks[new_chunk.index()], &mut self.chunk_groups);
        Some(new_chunk)
    }

    pub(crate) fn add_split_chunk(
        &mut self,
        source_chunks: &[ChunkHandle],
        name: Option<String>,
        root_modules: Vec<ModuleHandle>,
    ) -> ChunkHandle {
        let new_chunk = self.add_chunk(name, root_modules);
        for source_chunk in source_chunks {
            let original = self.chunks[source_chunk.index()].clone();
            original.split(&mut self.chunks[new_chunk.index()], &mut self.chunk_groups);
        }
        new_chunk
    }

    pub(crate) fn connect_chunk_and_module(&mut self, chunk: ChunkHandle, module: ModuleHandle) {
        if self.module_chunks.len() <= module.index() {
            self.module_chunks.resize_with(module.index() + 1, Vec::new);
            self.module_render_ids
                .resize_with(module.index() + 1, || None);
            self.module_hashes.resize_with(module.index() + 1, || None);
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

    pub(crate) fn disconnect_chunk_and_module(&mut self, chunk: ChunkHandle, module: ModuleHandle) {
        self.chunk_modules[chunk.index()].retain(|connected| *connected != module);
        if let Some(chunks) = self.module_chunks.get_mut(module.index()) {
            chunks.retain(|connected| *connected != chunk);
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

    pub(crate) fn module_render_id(&self, module: ModuleHandle) -> Option<&RenderId> {
        self.module_render_ids
            .get(module.index())
            .and_then(Option::as_ref)
    }

    pub(crate) fn module_references(
        &self,
        module_graph: &ModuleGraph,
        handle: ModuleHandle,
    ) -> ChunkGraphModuleReferences {
        let module = module_graph
            .module(handle)
            .expect("a Module Graph handle should address a Module");
        let mut chunk_render_ids = self
            .module_chunks(handle)
            .iter()
            .map(|chunk| {
                self.chunk(*chunk)
                    .expect("a Module Chunk reference must address an existing Chunk")
                    .render_id()
                    .clone()
            })
            .collect::<Vec<_>>();
        chunk_render_ids.sort();
        let outgoing_module_render_ids = module_graph
            .outgoing_connections(handle)
            .map(|connection| self.module_render_id(connection.module).cloned())
            .collect();
        let block_chunk_render_ids = module
            .blocks()
            .iter()
            .enumerate()
            .map(|(block_index, _)| {
                self.block_chunk_group(AsyncBlockOrigin {
                    module: handle,
                    block: crate::AsyncDependenciesBlockIndex::new(block_index),
                })
                .map(|group| {
                    self.chunk_groups()[group.index()]
                        .chunks()
                        .iter()
                        .map(|chunk| {
                            self.chunk(*chunk)
                                .expect("a Chunk Group must reference an existing Chunk")
                                .render_id()
                                .clone()
                        })
                        .collect()
                })
            })
            .collect();
        ChunkGraphModuleReferences {
            module_render_id: self.module_render_id(handle).cloned(),
            chunk_render_ids,
            outgoing_module_render_ids,
            block_chunk_render_ids,
        }
    }

    pub fn module_render_id_string(&self, module: ModuleHandle) -> Option<&str> {
        self.module_render_id(module).and_then(RenderId::as_string)
    }

    pub fn module_render_id_number(&self, module: ModuleHandle) -> Option<u32> {
        self.module_render_id(module).and_then(RenderId::as_number)
    }

    pub(crate) fn set_module_render_id(&mut self, module: ModuleHandle, render_id: RenderId) {
        if self.module_render_ids.len() <= module.index() {
            self.module_render_ids
                .resize_with(module.index() + 1, || None);
        }
        self.module_render_ids[module.index()] = Some(render_id);
    }

    pub(crate) fn set_module_hash(&mut self, module: ModuleHandle, module_hash: ModuleHash) {
        if self.module_hashes.len() <= module.index() {
            self.module_hashes.resize_with(module.index() + 1, || None);
        }
        self.module_hashes[module.index()] = Some(module_hash);
    }

    #[cfg(test)]
    pub(crate) fn module_hash(&self, module: ModuleHandle) -> Option<ModuleHash> {
        self.module_hashes.get(module.index()).copied().flatten()
    }

    pub(crate) fn set_chunk_render_id(&mut self, chunk: ChunkHandle, render_id: RenderId) {
        self.chunks[chunk.index()].assign_render_id(render_id);
    }

    pub fn block_chunk_group(&self, origin: AsyncBlockOrigin) -> Option<ChunkGroupHandle> {
        self.block_chunk_groups.get(&origin).copied()
    }

    pub fn chunk(&self, handle: ChunkHandle) -> Option<&Chunk> {
        self.chunks.get(handle.index())
    }

    #[cfg(test)]
    pub(crate) fn process_runtime_requirements(
        &mut self,
        module_requirements: impl IntoIterator<Item = (ModuleHandle, RuntimeRequirements)>,
    ) {
        let processed = module_requirements
            .into_iter()
            .map(|(module, direct)| (module, resolve_runtime_modules(&direct).0))
            .collect::<Vec<_>>();
        self.set_module_runtime_requirements(processed);
    }

    pub(crate) fn set_module_runtime_requirements(
        &mut self,
        module_requirements: impl IntoIterator<Item = (ModuleHandle, RuntimeRequirements)>,
    ) {
        self.module_runtime_requirements
            .resize_with(self.module_chunks.len(), RuntimeRequirements::default);
        for requirements in &mut self.module_runtime_requirements {
            *requirements = RuntimeRequirements::default();
        }
        for (module, processed) in module_requirements {
            assert!(
                module.index() < self.module_runtime_requirements.len(),
                "Runtime Requirements must reference a Module in the Chunk Graph"
            );
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
            let mut visited = FxHashSet::default();
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
