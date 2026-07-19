// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/optimize/ModuleConcatenationPlugin.js

use std::collections::BTreeSet;

use rustc_hash::FxHashSet;

use crate::{
    ChunkGraph, Compilation, ModuleGraph, ModuleHandle, ModuleType,
    compilation::CompilationHookSet, compiler::CompilerHookSet,
    optimize::concatenated_module::ConcatenatedModule,
};

pub(crate) struct ModuleConcatenationPlugin;

impl ModuleConcatenationPlugin {
    pub(crate) fn apply(&self, hooks: &mut CompilerHookSet) {
        hooks.compilation.tap(
            "ModuleConcatenationPlugin",
            |compilation_hooks: &mut CompilationHookSet| {
                compilation_hooks
                    .optimize_chunk_modules
                    .tap("ModuleConcatenationPlugin", optimize_chunk_modules);
            },
        );
    }
}

fn optimize_chunk_modules(compilation: &mut Compilation) {
    let entries = compilation
        .entries()
        .iter()
        .copied()
        .collect::<FxHashSet<_>>();
    let configurations = find_configurations(
        compilation.module_graph(),
        compilation.chunk_graph(),
        &entries,
    );
    let mut used_modules = FxHashSet::default();
    for configuration in configurations {
        if used_modules.contains(&configuration.root) {
            continue;
        }
        used_modules.extend(configuration.modules.iter().copied());
        let concatenated_module = ConcatenatedModule::new(
            configuration.root,
            configuration.modules.into_iter().collect(),
            compilation.module_graph(),
        );
        compilation
            .chunk_graph_mut()
            .add_concatenated_module(concatenated_module);
    }
}

#[derive(Debug)]
struct ConcatenationConfiguration {
    root: ModuleHandle,
    modules: BTreeSet<ModuleHandle>,
}

fn find_configurations(
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    entries: &FxHashSet<ModuleHandle>,
) -> Vec<ConcatenationConfiguration> {
    let possible_inners = module_graph
        .modules()
        .iter()
        .filter(|module| {
            module_can_concatenate(module_graph, chunk_graph, module.handle())
                && !entries.contains(&module.handle())
        })
        .map(|module| module.handle())
        .collect::<FxHashSet<_>>();
    let mut roots = module_graph
        .modules()
        .iter()
        .filter(|module| {
            module_can_concatenate(module_graph, chunk_graph, module.handle())
                && module_graph
                    .exports_info(module.handle())
                    .provided_exports()
                    .is_some()
        })
        .map(|module| module.handle())
        .collect::<Vec<_>>();
    roots.sort_by(|left, right| compare_module_identity(module_graph, *left, *right));

    let mut used_as_inner = FxHashSet::default();
    let mut configurations = Vec::new();
    for root in roots {
        if used_as_inner.contains(&root) {
            continue;
        }
        let root_chunks = chunk_graph.module_chunks(root).to_vec();
        let mut modules = BTreeSet::from([root]);
        let mut candidates = imports(module_graph, root);
        let mut processed = FxHashSet::default();
        while let Some(candidate) = candidates
            .iter()
            .min_by(|left, right| compare_module_identity(module_graph, **left, **right))
            .copied()
        {
            candidates.remove(&candidate);
            if !processed.insert(candidate) {
                continue;
            }
            let snapshot = modules.clone();
            if try_to_add(
                module_graph,
                chunk_graph,
                &root_chunks,
                &possible_inners,
                &mut modules,
                candidate,
            ) {
                let newly_added = modules.difference(&snapshot).copied().collect::<Vec<_>>();
                for added in newly_added {
                    candidates.extend(imports(module_graph, added));
                }
            }
        }
        if modules.len() > 1 {
            used_as_inner.extend(modules.iter().copied().filter(|module| *module != root));
            configurations.push(ConcatenationConfiguration { root, modules });
        }
    }

    configurations.sort_by(|left, right| {
        right
            .modules
            .len()
            .cmp(&left.modules.len())
            .then_with(|| compare_module_identity(module_graph, left.root, right.root))
    });
    configurations
}

fn try_to_add(
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    root_chunks: &[crate::ChunkHandle],
    possible_inners: &FxHashSet<ModuleHandle>,
    modules: &mut BTreeSet<ModuleHandle>,
    candidate: ModuleHandle,
) -> bool {
    if modules.contains(&candidate) {
        return true;
    }
    if !possible_inners.contains(&candidate)
        || root_chunks
            .iter()
            .any(|chunk| !chunk_graph.chunk_modules(*chunk).contains(&candidate))
    {
        return false;
    }

    let incoming = module_graph
        .incoming_connections(candidate)
        .filter(|connection| connection.is_active())
        .filter_map(|connection| {
            connection
                .origin_module
                .map(|origin| (origin, connection.dependency.can_concatenate()))
        })
        .filter(|(origin, _)| !chunk_graph.module_chunks(*origin).is_empty())
        .collect::<Vec<_>>();
    if incoming.iter().any(|(origin, can_concatenate)| {
        !can_concatenate
            || root_chunks
                .iter()
                .any(|chunk| !chunk_graph.chunk_modules(*chunk).contains(origin))
    }) {
        return false;
    }

    let snapshot = modules.clone();
    modules.insert(candidate);
    let mut importers = incoming
        .into_iter()
        .map(|(origin, _)| origin)
        .collect::<Vec<_>>();
    importers.sort_by(|left, right| compare_module_identity(module_graph, *left, *right));
    importers.dedup();
    for importer in importers {
        if !try_to_add(
            module_graph,
            chunk_graph,
            root_chunks,
            possible_inners,
            modules,
            importer,
        ) {
            *modules = snapshot;
            return false;
        }
    }
    true
}

fn imports(module_graph: &ModuleGraph, module: ModuleHandle) -> BTreeSet<ModuleHandle> {
    module_graph
        .outgoing_connections(module)
        .filter(|connection| connection.is_active() && connection.dependency.can_concatenate())
        .map(|connection| connection.module)
        .collect()
}

fn module_can_concatenate(
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    handle: ModuleHandle,
) -> bool {
    let Some(module) = module_graph.module(handle) else {
        return false;
    };
    module.identity().module_type == ModuleType::JavaScriptAuto
        && module.is_harmony()
        && module.build_error().is_none()
        && !chunk_graph.module_chunks(handle).is_empty()
}

fn compare_module_identity(
    module_graph: &ModuleGraph,
    left: ModuleHandle,
    right: ModuleHandle,
) -> std::cmp::Ordering {
    module_graph
        .module(left)
        .expect("a concatenation candidate must exist in the Module Graph")
        .identity()
        .cmp(
            module_graph
                .module(right)
                .expect("a concatenation candidate must exist in the Module Graph")
                .identity(),
        )
}
