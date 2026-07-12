use std::{
    cmp::Ordering,
    collections::{HashMap, VecDeque},
};

use crate::{
    AsyncDependenciesBlockIndex, CompilerOptions, ModuleGraph, ModuleHandle,
    chunk_graph::ChunkGraph,
    chunk_group::{AsyncBlockOrigin, ChunkGroupHandle, ChunkGroupKind},
};

const MODULES_PER_MASK_WORD: usize = u64::BITS as usize;

// ModuleHandle values are dense arena handles, so webpack's available-module mask
// maps directly to compact word-indexed storage without an ordinal HashMap.
#[derive(Clone, PartialEq, Eq)]
struct ModuleMask {
    words: Box<[u64]>,
}

impl ModuleMask {
    fn new(module_count: usize) -> Self {
        let word_count = module_count.div_ceil(MODULES_PER_MASK_WORD);
        Self {
            words: vec![0; word_count].into_boxed_slice(),
        }
    }

    fn from_modules(module_count: usize, modules: impl IntoIterator<Item = ModuleHandle>) -> Self {
        let mut mask = Self::new(module_count);
        for module in modules {
            mask.insert(module);
        }
        mask
    }

    fn insert(&mut self, module: ModuleHandle) -> bool {
        let word = self
            .words
            .get_mut(module.index() / MODULES_PER_MASK_WORD)
            .expect("Module Mask must be sized for every Module in the Module Graph");
        let bit = 1 << (module.index() % MODULES_PER_MASK_WORD);
        let changed = *word & bit == 0;
        *word |= bit;
        changed
    }

    fn contains(&self, module: ModuleHandle) -> bool {
        self.words
            .get(module.index() / MODULES_PER_MASK_WORD)
            .is_some_and(|word| word & (1 << (module.index() % MODULES_PER_MASK_WORD)) != 0)
    }

    #[cfg(test)]
    fn union_with(&mut self, other: &Self) {
        assert_eq!(self.words.len(), other.words.len());
        for (word, other_word) in self.words.iter_mut().zip(&other.words) {
            *word |= other_word;
        }
    }

    fn intersect_with(&mut self, other: &Self) {
        assert_eq!(self.words.len(), other.words.len());
        for (word, other_word) in self.words.iter_mut().zip(&other.words) {
            *word &= other_word;
        }
    }
}

struct EntrypointAsyncPlan {
    group: ChunkGroupHandle,
    resulting_available_modules: ModuleMask,
    modules: Vec<ModuleHandle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LogicalChunkGroup {
    Entrypoint(usize),
    AsyncChunk(ModuleHandle),
}

struct AsyncParentPlan {
    resulting_available_modules: ModuleMask,
    origins: Vec<AsyncBlockOrigin>,
}

struct AsyncChunkPlan {
    target: ModuleHandle,
    static_modules: Vec<ModuleHandle>,
    min_available_modules: ModuleMask,
    resulting_available_modules: ModuleMask,
    parents: HashMap<LogicalChunkGroup, AsyncParentPlan>,
}

impl AsyncChunkPlan {
    fn new(
        target: ModuleHandle,
        static_modules: Vec<ModuleHandle>,
        parent: LogicalChunkGroup,
        parent_resulting_available_modules: &ModuleMask,
        origin: AsyncBlockOrigin,
    ) -> Self {
        let min_available_modules = parent_resulting_available_modules.clone();
        let mut resulting_available_modules = min_available_modules.clone();
        for module in &static_modules {
            resulting_available_modules.insert(*module);
        }
        Self {
            target,
            static_modules,
            min_available_modules,
            resulting_available_modules,
            parents: HashMap::from([(
                parent,
                AsyncParentPlan {
                    resulting_available_modules: parent_resulting_available_modules.clone(),
                    origins: vec![origin],
                },
            )]),
        }
    }

    fn add_parent(
        &mut self,
        parent: LogicalChunkGroup,
        parent_resulting_available_modules: &ModuleMask,
        origin: AsyncBlockOrigin,
    ) -> bool {
        let parent_plan = self
            .parents
            .entry(parent)
            .or_insert_with(|| AsyncParentPlan {
                resulting_available_modules: parent_resulting_available_modules.clone(),
                origins: Vec::new(),
            });
        parent_plan.resulting_available_modules = parent_resulting_available_modules.clone();
        if !parent_plan.origins.contains(&origin) {
            parent_plan.origins.push(origin);
        }

        let mut parents = self.parents.values();
        let mut min_available_modules = parents
            .next()
            .map(|parent| parent.resulting_available_modules.clone())
            .expect("Async Chunk Plan must retain at least one parent");
        for parent in parents {
            min_available_modules.intersect_with(&parent.resulting_available_modules);
        }
        let mut resulting_available_modules = min_available_modules.clone();
        for module in &self.static_modules {
            resulting_available_modules.insert(*module);
        }
        let changed = self.min_available_modules != min_available_modules
            || self.resulting_available_modules != resulting_available_modules;
        self.min_available_modules = min_available_modules;
        self.resulting_available_modules = resulting_available_modules;
        changed
    }
}

pub(crate) fn build_chunk_graph(
    options: &CompilerOptions,
    module_graph: &ModuleGraph,
    entries: &[ModuleHandle],
) -> ChunkGraph {
    let mut chunk_graph = ChunkGraph::default();
    let module_count = module_graph.modules().len();
    let mut entrypoint_plans = Vec::new();
    for (entry_index, entry_module) in entries.iter().copied().enumerate() {
        let entry_name = options
            .entries
            .get(entry_index)
            .map(|entry| entry.name.clone())
            .unwrap_or_else(|| format!("entry{entry_index}"));
        let entry_chunk = chunk_graph.add_chunk(Some(entry_name.clone()), vec![entry_module]);
        let entry_group = chunk_graph.add_chunk_group(
            ChunkGroupKind::Entrypoint {
                name: entry_name.clone(),
            },
            None,
        );
        chunk_graph.connect_chunk_and_group(entry_chunk, entry_group);
        chunk_graph.add_entrypoint(entry_group);

        let initial_modules = collect_static_reachable(module_graph, entry_module);
        for module in &initial_modules {
            chunk_graph.connect_chunk_and_module(entry_chunk, *module);
        }

        entrypoint_plans.push(EntrypointAsyncPlan {
            group: entry_group,
            resulting_available_modules: ModuleMask::from_modules(
                module_count,
                initial_modules.iter().copied(),
            ),
            modules: initial_modules,
        });
    }

    // The implemented staging model reuses one Async Chunk plan per target Module.
    // This is intentionally narrower than webpack's AsyncDependenciesBlock-first
    // ChunkGroupInfo model and is recorded in the implementation differences.
    let mut async_chunk_plans = HashMap::<ModuleHandle, AsyncChunkPlan>::new();
    let mut pending = (0..entrypoint_plans.len())
        .map(LogicalChunkGroup::Entrypoint)
        .collect::<VecDeque<_>>();
    while let Some(parent) = pending.pop_front() {
        let (parent_modules, parent_resulting_available_modules) = match parent {
            LogicalChunkGroup::Entrypoint(index) => (
                entrypoint_plans[index].modules.clone(),
                entrypoint_plans[index].resulting_available_modules.clone(),
            ),
            LogicalChunkGroup::AsyncChunk(target) => {
                let plan = async_chunk_plans
                    .get(&target)
                    .expect("pending Async Chunk Plan must exist");
                (
                    plan.static_modules
                        .iter()
                        .filter(|module| !plan.min_available_modules.contains(**module))
                        .copied()
                        .collect(),
                    plan.resulting_available_modules.clone(),
                )
            }
        };

        for (origin, target) in dynamic_import_origins(module_graph, parent_modules.iter().copied())
        {
            if parent_resulting_available_modules.contains(target) {
                continue;
            }
            if let Some(plan) = async_chunk_plans.get_mut(&target) {
                if plan.add_parent(parent, &parent_resulting_available_modules, origin) {
                    pending.push_back(LogicalChunkGroup::AsyncChunk(target));
                }
            } else {
                let static_modules = collect_static_reachable(module_graph, target);
                async_chunk_plans.insert(
                    target,
                    AsyncChunkPlan::new(
                        target,
                        static_modules,
                        parent,
                        &parent_resulting_available_modules,
                        origin,
                    ),
                );
                pending.push_back(LogicalChunkGroup::AsyncChunk(target));
            }
        }
    }

    let mut ordered_targets = async_chunk_plans.keys().copied().collect::<Vec<_>>();
    ordered_targets.sort_by(|left, right| compare_module_identities(module_graph, *left, *right));

    let mut chunk_groups_by_target = HashMap::new();
    for target in &ordered_targets {
        let plan = &async_chunk_plans[target];
        let origin = plan
            .parents
            .values()
            .flat_map(|parent| parent.origins.iter().copied())
            .min_by(|left, right| compare_async_origins(module_graph, *left, *right))
            .expect("Async Chunk Plan must have at least one parent origin");
        let chunk = chunk_graph.add_chunk(None, vec![plan.target]);
        let group = chunk_graph.add_chunk_group(ChunkGroupKind::Async, Some(origin));
        chunk_graph.connect_chunk_and_group(chunk, group);
        for module in plan
            .static_modules
            .iter()
            .filter(|module| !plan.min_available_modules.contains(**module))
        {
            chunk_graph.connect_chunk_and_module(chunk, *module);
        }
        chunk_groups_by_target.insert(*target, group);
    }

    for target in ordered_targets {
        let child_group = chunk_groups_by_target[&target];
        let plan = &async_chunk_plans[&target];
        let mut parents = plan.parents.iter().collect::<Vec<_>>();
        parents
            .sort_by(|(left, _), (right, _)| compare_logical_groups(module_graph, **left, **right));
        for (parent, parent_plan) in parents {
            let parent_group = match parent {
                LogicalChunkGroup::Entrypoint(index) => entrypoint_plans[*index].group,
                LogicalChunkGroup::AsyncChunk(target) => chunk_groups_by_target[target],
            };
            if !chunk_group_reaches(&chunk_graph, child_group, parent_group) {
                chunk_graph.connect_chunk_groups(parent_group, child_group);
            } else {
                chunk_graph.connect_runtime_chunk_groups(parent_group, child_group);
            }
            for origin in &parent_plan.origins {
                chunk_graph.connect_block_and_chunk_group(*origin, child_group);
            }
        }
    }

    chunk_graph
}

fn collect_static_reachable(module_graph: &ModuleGraph, start: ModuleHandle) -> Vec<ModuleHandle> {
    let mut visited = ModuleMask::new(module_graph.modules().len());
    let mut queue = VecDeque::from([start]);
    let mut modules = Vec::new();

    while let Some(module) = queue.pop_front() {
        if !visited.insert(module) {
            continue;
        }
        modules.push(module);

        for connection in module_graph.outgoing_connections(module) {
            if connection.origin_block.is_none()
                && connection.dependency.is_static_module_dependency()
                && connection.is_active()
            {
                queue.push_back(connection.module);
            }
        }
    }

    modules
}

fn dynamic_import_origins(
    module_graph: &ModuleGraph,
    modules: impl IntoIterator<Item = ModuleHandle>,
) -> Vec<(AsyncBlockOrigin, ModuleHandle)> {
    let mut origins = Vec::new();
    for module in modules {
        let module_ref = module_graph
            .module(module)
            .expect("Chunk planning must reference an existing Module");
        for (block_index, block) in module_ref.blocks().iter().enumerate() {
            if !block
                .dependencies()
                .iter()
                .any(|dependency| dependency.is_import_dependency())
            {
                continue;
            }
            let origin = AsyncBlockOrigin {
                module,
                block: AsyncDependenciesBlockIndex::new(block_index),
            };
            if let Some(target) = import_block_target(module_graph, origin) {
                origins.push((origin, target));
            }
        }
    }
    origins.sort_by(|(left_origin, left_target), (right_origin, right_target)| {
        compare_async_origins(module_graph, *left_origin, *right_origin)
            .then_with(|| compare_module_identities(module_graph, *left_target, *right_target))
    });
    origins.dedup();
    origins
}

fn compare_async_origins(
    module_graph: &ModuleGraph,
    left: AsyncBlockOrigin,
    right: AsyncBlockOrigin,
) -> Ordering {
    compare_module_identities(module_graph, left.module, right.module)
        .then(left.block.cmp(&right.block))
}

fn compare_logical_groups(
    module_graph: &ModuleGraph,
    left: LogicalChunkGroup,
    right: LogicalChunkGroup,
) -> Ordering {
    match (left, right) {
        (LogicalChunkGroup::Entrypoint(left), LogicalChunkGroup::Entrypoint(right)) => {
            left.cmp(&right)
        }
        (LogicalChunkGroup::Entrypoint(_), LogicalChunkGroup::AsyncChunk(_)) => Ordering::Less,
        (LogicalChunkGroup::AsyncChunk(_), LogicalChunkGroup::Entrypoint(_)) => Ordering::Greater,
        (LogicalChunkGroup::AsyncChunk(left), LogicalChunkGroup::AsyncChunk(right)) => {
            compare_module_identities(module_graph, left, right)
        }
    }
}

fn compare_module_identities(
    module_graph: &ModuleGraph,
    left: ModuleHandle,
    right: ModuleHandle,
) -> Ordering {
    let left_identity = module_graph
        .module(left)
        .expect("Chunk planning order must reference an existing Module")
        .identity();
    let right_identity = module_graph
        .module(right)
        .expect("Chunk planning order must reference an existing Module")
        .identity();
    left_identity.cmp(right_identity)
}

fn chunk_group_reaches(
    graph: &ChunkGraph,
    start: ChunkGroupHandle,
    target: ChunkGroupHandle,
) -> bool {
    let mut visited = vec![false; graph.chunk_groups().len()];
    let mut pending = VecDeque::from([start]);
    while let Some(group) = pending.pop_front() {
        if group == target {
            return true;
        }
        if visited[group.index()] {
            continue;
        }
        visited[group.index()] = true;
        pending.extend(
            graph.chunk_groups()[group.index()]
                .children()
                .iter()
                .copied(),
        );
    }
    false
}

fn import_block_target(
    module_graph: &ModuleGraph,
    origin: AsyncBlockOrigin,
) -> Option<ModuleHandle> {
    module_graph
        .outgoing_connections(origin.module)
        .find(|connection| {
            connection.origin_block == Some(origin.block)
                && connection.dependency.is_import_dependency()
        })
        .map(|connection| connection.module)
}

#[cfg(test)]
mod tests {
    use super::ModuleMask;
    use crate::ModuleHandle;

    #[test]
    fn module_mask_uses_dense_module_handles_across_word_boundaries() {
        let mut mask = ModuleMask::new(130);

        for index in [0, 63, 64, 129] {
            assert!(mask.insert(ModuleHandle::new(index)));
            assert!(!mask.insert(ModuleHandle::new(index)));
        }

        for index in 0..130 {
            assert_eq!(
                mask.contains(ModuleHandle::new(index)),
                [0, 63, 64, 129].contains(&index)
            );
        }
    }

    #[test]
    fn module_mask_intersection_and_union_match_available_module_algebra() {
        let mut left =
            ModuleMask::from_modules(130, [0, 63, 64, 100].into_iter().map(ModuleHandle::new));
        let right =
            ModuleMask::from_modules(130, [1, 63, 64, 129].into_iter().map(ModuleHandle::new));

        let mut intersection = left.clone();
        intersection.intersect_with(&right);
        assert_eq!(
            (0..130)
                .filter(|index| intersection.contains(ModuleHandle::new(*index)))
                .collect::<Vec<_>>(),
            [63, 64]
        );

        left.union_with(&right);
        assert_eq!(
            (0..130)
                .filter(|index| left.contains(ModuleHandle::new(*index)))
                .collect::<Vec<_>>(),
            [0, 1, 63, 64, 100, 129]
        );
    }
}
