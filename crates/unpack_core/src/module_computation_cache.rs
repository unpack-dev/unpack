//! Compiler-owned memoization for computations on unaffected modules.
//!
//! Unlike the record cache, these entries are never persisted or promoted
//! through cache layers. They are validated against the current Module Graph
//! and live only as long as the Compiler.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use crate::{
    ChunkGraph, ExportsInfo, ModuleGraph, ModuleHandle, ModuleIdentity,
    chunk_graph::{ChunkGraphModuleReferences, ModuleHash},
    runtime::RuntimeRequirements,
};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Debug, Clone, Default)]
pub(crate) struct ModuleComputationCache {
    state: Arc<Mutex<ModuleComputationCacheState>>,
}

#[derive(Debug, Default)]
struct ModuleComputationCacheState {
    modules: FxHashMap<ModuleIdentity, ModuleComputationEntry>,
    current_handles: FxHashMap<ModuleIdentity, ModuleHandle>,
    stats: ModuleComputationCacheStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreChunkGraphSignature {
    source_hash: u64,
    build_error: Option<String>,
    references: Vec<PreChunkGraphModuleReference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreChunkGraphModuleReference {
    block: Option<usize>,
    dependency: Option<usize>,
    target: ModuleIdentity,
}

#[derive(Debug)]
struct ModuleComputationEntry {
    pre_chunk_graph: PreChunkGraphModuleComputationEntry,
    post_id_assignment: Option<PostIdAssignmentModuleComputationEntry>,
}

// Corresponds to webpack's `moduleMemCaches`: these memos are validated after
// Module Graph construction and before `finishModules` and Chunk Graph work.
#[derive(Debug)]
struct PreChunkGraphModuleComputationEntry {
    signature: PreChunkGraphSignature,
    provided_exports: Option<ExportsInfo>,
    static_reachable: Option<Vec<ModuleIdentity>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PostIdAssignmentSignature {
    references: ChunkGraphModuleReferences,
    exports_info: ExportsInfo,
}

// Corresponds to webpack's `moduleMemCaches2`: these memos are only valid after
// ID Assignment and use the current Chunk Graph references for validation.
#[derive(Debug)]
struct PostIdAssignmentModuleComputationEntry {
    signature: PostIdAssignmentSignature,
    runtime_requirements: Option<RuntimeRequirements>,
    module_hash: Option<ModuleHash>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ModuleComputationCacheStats {
    pub(crate) provided_exports_hits: usize,
    pub(crate) provided_exports_misses: usize,
    pub(crate) pre_chunk_graph_invalidated_modules: usize,
    pub(crate) static_reachable_hits: usize,
    pub(crate) static_reachable_misses: usize,
    pub(crate) runtime_requirements_hits: usize,
    pub(crate) runtime_requirements_misses: usize,
    pub(crate) module_hash_hits: usize,
    pub(crate) module_hash_misses: usize,
    pub(crate) post_id_assignment_invalidated_modules: usize,
}

impl ModuleComputationCache {
    pub(crate) fn prepare_before_chunk_graph(&self, module_graph: &ModuleGraph) {
        let signatures = module_graph
            .modules()
            .iter()
            .map(|module| {
                (
                    module.identity().clone(),
                    module.handle(),
                    pre_chunk_graph_signature(module_graph, module.handle()),
                )
            })
            .collect::<Vec<_>>();
        let current_identities = signatures
            .iter()
            .map(|(identity, _, _)| identity.clone())
            .collect::<FxHashSet<_>>();
        let mut state = self
            .state
            .lock()
            .expect("module computation cache mutex should not be poisoned");
        state
            .modules
            .retain(|identity, _| current_identities.contains(identity));
        state.current_handles = signatures
            .iter()
            .map(|(identity, handle, _)| (identity.clone(), *handle))
            .collect();

        let mut affected = signatures
            .iter()
            .filter_map(|(identity, handle, signature)| {
                let unchanged = state
                    .modules
                    .get(identity)
                    .is_some_and(|entry| entry.pre_chunk_graph.signature == *signature);
                (!unchanged).then_some(*handle)
            })
            .collect::<FxHashSet<_>>();
        let mut queue = affected.iter().copied().collect::<VecDeque<_>>();
        while let Some(handle) = queue.pop_front() {
            for connection in module_graph.incoming_connections(handle) {
                if let Some(origin) = connection.origin_module
                    && affected.insert(origin)
                {
                    queue.push_back(origin);
                }
            }
        }

        for (identity, handle, signature) in signatures {
            let is_affected = affected.contains(&handle);
            let invalidated = {
                let entry =
                    state
                        .modules
                        .entry(identity)
                        .or_insert_with(|| ModuleComputationEntry {
                            pre_chunk_graph: PreChunkGraphModuleComputationEntry {
                                signature: signature.clone(),
                                provided_exports: None,
                                static_reachable: None,
                            },
                            post_id_assignment: None,
                        });
                if is_affected {
                    let provided_exports_invalidated =
                        entry.pre_chunk_graph.provided_exports.take().is_some();
                    let static_reachable_invalidated =
                        entry.pre_chunk_graph.static_reachable.take().is_some();
                    // Pre-Chunk-Graph invalidation always dominates the later stage.
                    let post_id_assignment_invalidated = entry.post_id_assignment.take().is_some();
                    let invalidated = provided_exports_invalidated
                        || static_reachable_invalidated
                        || post_id_assignment_invalidated;
                    entry.pre_chunk_graph.signature = signature;
                    invalidated
                } else {
                    false
                }
            };
            if invalidated {
                state.stats.pre_chunk_graph_invalidated_modules += 1;
            }
        }
    }

    pub(crate) fn prepare_after_id_assignment(
        &self,
        module_graph: &ModuleGraph,
        chunk_graph: &ChunkGraph,
    ) {
        let signatures = module_graph
            .modules()
            .iter()
            .map(|module| {
                (
                    module.identity().clone(),
                    post_id_assignment_signature(module_graph, chunk_graph, module.handle()),
                )
            })
            .collect::<Vec<_>>();
        let mut state = self
            .state
            .lock()
            .expect("module computation cache mutex should not be poisoned");
        for (identity, signature) in signatures {
            let entry = state
                .modules
                .get_mut(&identity)
                .expect("Pre-Chunk-Graph entry must be prepared before ID Assignment");
            let changed = entry
                .post_id_assignment
                .as_ref()
                .is_some_and(|cached| cached.signature != signature);
            if entry.post_id_assignment.is_none() || changed {
                // A later-only change replaces this entry without clearing
                // the Pre-Chunk-Graph memos stored beside it.
                entry.post_id_assignment = Some(PostIdAssignmentModuleComputationEntry {
                    signature,
                    runtime_requirements: None,
                    module_hash: None,
                });
            }
            if changed {
                state.stats.post_id_assignment_invalidated_modules += 1;
            }
        }
    }

    pub(crate) fn get_provided_exports(&self, identity: &ModuleIdentity) -> Option<ExportsInfo> {
        let mut state = self
            .state
            .lock()
            .expect("module computation cache mutex should not be poisoned");
        let result = state
            .modules
            .get(identity)
            .and_then(|entry| entry.pre_chunk_graph.provided_exports.clone());
        if result.is_some() {
            state.stats.provided_exports_hits += 1;
        } else {
            state.stats.provided_exports_misses += 1;
        }
        result
    }

    pub(crate) fn store_provided_exports(
        &self,
        identity: &ModuleIdentity,
        exports_info: ExportsInfo,
    ) {
        self.state
            .lock()
            .expect("module computation cache mutex should not be poisoned")
            .modules
            .get_mut(identity)
            .expect("Module Computation Cache must be prepared before storing a memo")
            .pre_chunk_graph
            .provided_exports = Some(exports_info);
    }

    pub(crate) fn get_static_reachable(
        &self,
        identity: &ModuleIdentity,
    ) -> Option<Vec<ModuleHandle>> {
        let mut state = self
            .state
            .lock()
            .expect("module computation cache mutex should not be poisoned");
        let identities = state
            .modules
            .get(identity)
            .and_then(|entry| entry.pre_chunk_graph.static_reachable.clone());
        let result = identities.and_then(|identities| {
            identities
                .iter()
                .map(|identity| state.current_handles.get(identity).copied())
                .collect()
        });
        if result.is_some() {
            state.stats.static_reachable_hits += 1;
        } else {
            state.stats.static_reachable_misses += 1;
        }
        result
    }

    pub(crate) fn store_static_reachable(
        &self,
        identity: &ModuleIdentity,
        modules: &[ModuleHandle],
        module_graph: &ModuleGraph,
    ) {
        let identities = modules
            .iter()
            .map(|handle| {
                module_graph
                    .module(*handle)
                    .expect("a cached reachable Module must exist in the Module Graph")
                    .identity()
                    .clone()
            })
            .collect();
        self.state
            .lock()
            .expect("module computation cache mutex should not be poisoned")
            .modules
            .get_mut(identity)
            .expect("Module Computation Cache must be prepared before storing a memo")
            .pre_chunk_graph
            .static_reachable = Some(identities);
    }

    pub(crate) fn get_runtime_requirements(
        &self,
        identity: &ModuleIdentity,
    ) -> Option<RuntimeRequirements> {
        let mut state = self
            .state
            .lock()
            .expect("module computation cache mutex should not be poisoned");
        let result = state
            .modules
            .get(identity)
            .and_then(|entry| entry.post_id_assignment.as_ref())
            .and_then(|entry| entry.runtime_requirements);
        if result.is_some() {
            state.stats.runtime_requirements_hits += 1;
        } else {
            state.stats.runtime_requirements_misses += 1;
        }
        result
    }

    pub(crate) fn store_runtime_requirements(
        &self,
        identity: &ModuleIdentity,
        runtime_requirements: RuntimeRequirements,
    ) {
        self.state
            .lock()
            .expect("module computation cache mutex should not be poisoned")
            .modules
            .get_mut(identity)
            .and_then(|entry| entry.post_id_assignment.as_mut())
            .expect("Post-ID-Assignment cache must be prepared before storing a memo")
            .runtime_requirements = Some(runtime_requirements);
    }

    pub(crate) fn get_module_hash(&self, identity: &ModuleIdentity) -> Option<ModuleHash> {
        let mut state = self
            .state
            .lock()
            .expect("module computation cache mutex should not be poisoned");
        let result = state
            .modules
            .get(identity)
            .and_then(|entry| entry.post_id_assignment.as_ref())
            .and_then(|entry| entry.module_hash);
        if result.is_some() {
            state.stats.module_hash_hits += 1;
        } else {
            state.stats.module_hash_misses += 1;
        }
        result
    }

    pub(crate) fn store_module_hash(&self, identity: &ModuleIdentity, module_hash: ModuleHash) {
        self.state
            .lock()
            .expect("module computation cache mutex should not be poisoned")
            .modules
            .get_mut(identity)
            .and_then(|entry| entry.post_id_assignment.as_mut())
            .expect("Post-ID-Assignment cache must be prepared before storing a memo")
            .module_hash = Some(module_hash);
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> ModuleComputationCacheStats {
        self.state
            .lock()
            .expect("module computation cache mutex should not be poisoned")
            .stats
    }
}

fn post_id_assignment_signature(
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    handle: ModuleHandle,
) -> PostIdAssignmentSignature {
    PostIdAssignmentSignature {
        references: chunk_graph.module_references(module_graph, handle),
        exports_info: module_graph.exports_info(handle).clone(),
    }
}

fn pre_chunk_graph_signature(
    module_graph: &ModuleGraph,
    handle: ModuleHandle,
) -> PreChunkGraphSignature {
    let module = module_graph
        .module(handle)
        .expect("a Module Graph handle should address a Module");
    let references = module_graph
        .outgoing_connections(handle)
        .map(|connection| {
            let target = module_graph
                .module(connection.module)
                .expect("a Module Graph connection should target a Module");
            PreChunkGraphModuleReference {
                block: connection.origin_block.map(|block| block.index()),
                dependency: connection
                    .origin_dependency_index
                    .map(|dependency| dependency.index()),
                target: target.identity().clone(),
            }
        })
        .collect();
    PreChunkGraphSignature {
        source_hash: module.source_hash(),
        build_error: module.build_error().map(ToString::to_string),
        references,
    }
}
