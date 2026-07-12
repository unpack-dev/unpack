//! Compiler-owned memoization for computations on unaffected modules.
//!
//! Unlike the record cache, these entries are never persisted or promoted
//! through cache layers. They are validated against the current Module Graph
//! and live only as long as the Compiler.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};

use crate::{ExportsInfo, ModuleGraph, ModuleHandle, ModuleIdentity};

#[derive(Debug, Clone, Default)]
pub(crate) struct ModuleComputationCache {
    state: Arc<Mutex<ModuleComputationCacheState>>,
}

#[derive(Debug, Default)]
struct ModuleComputationCacheState {
    modules: HashMap<ModuleIdentity, ModuleComputationEntry>,
    current_handles: HashMap<ModuleIdentity, ModuleHandle>,
    stats: ModuleComputationCacheStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModuleSignature {
    source_hash: u64,
    build_error: Option<String>,
    references: Vec<ModuleReference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModuleReference {
    block: Option<usize>,
    dependency: Option<usize>,
    target: ModuleIdentity,
}

#[derive(Debug)]
struct ModuleComputationEntry {
    signature: ModuleSignature,
    provided_exports: Option<ExportsInfo>,
    static_reachable: Option<Vec<ModuleIdentity>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ModuleComputationCacheStats {
    pub(crate) provided_exports_hits: usize,
    pub(crate) provided_exports_misses: usize,
    pub(crate) invalidated_modules: usize,
    pub(crate) static_reachable_hits: usize,
    pub(crate) static_reachable_misses: usize,
}

impl ModuleComputationCache {
    pub(crate) fn prepare(&self, module_graph: &ModuleGraph) {
        let signatures = module_graph
            .modules()
            .iter()
            .map(|module| {
                (
                    module.identity().clone(),
                    module.handle(),
                    module_signature(module_graph, module.handle()),
                )
            })
            .collect::<Vec<_>>();
        let current_identities = signatures
            .iter()
            .map(|(identity, _, _)| identity.clone())
            .collect::<HashSet<_>>();
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
                    .is_some_and(|entry| entry.signature == *signature);
                (!unchanged).then_some(*handle)
            })
            .collect::<HashSet<_>>();
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
                            signature: signature.clone(),
                            provided_exports: None,
                            static_reachable: None,
                        });
                if is_affected {
                    let provided_exports_invalidated = entry.provided_exports.take().is_some();
                    let static_reachable_invalidated = entry.static_reachable.take().is_some();
                    let invalidated = provided_exports_invalidated || static_reachable_invalidated;
                    entry.signature = signature;
                    invalidated
                } else {
                    false
                }
            };
            if invalidated {
                state.stats.invalidated_modules += 1;
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
            .and_then(|entry| entry.provided_exports.clone());
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
            .and_then(|entry| entry.static_reachable.clone());
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
            .static_reachable = Some(identities);
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> ModuleComputationCacheStats {
        self.state
            .lock()
            .expect("module computation cache mutex should not be poisoned")
            .stats
    }
}

fn module_signature(module_graph: &ModuleGraph, handle: ModuleHandle) -> ModuleSignature {
    let module = module_graph
        .module(handle)
        .expect("a Module Graph handle should address a Module");
    let references = module_graph
        .outgoing_connections(handle)
        .map(|connection| {
            let target = module_graph
                .module(connection.module)
                .expect("a Module Graph connection should target a Module");
            ModuleReference {
                block: connection.origin_block.map(|block| block.index()),
                dependency: connection
                    .origin_dependency_index
                    .map(|dependency| dependency.index()),
                target: target.identity().clone(),
            }
        })
        .collect();
    ModuleSignature {
        source_hash: module.source_hash(),
        build_error: module.build_error().map(ToString::to_string),
        references,
    }
}
