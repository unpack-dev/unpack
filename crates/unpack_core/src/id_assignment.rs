// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/ids/IdHelpers.js

use std::{
    collections::{BTreeMap, HashSet},
    fmt,
    path::{Path, PathBuf},
};

use crate::{ChunkGraph, CompilerOptions, ModuleGraph, cache_hash::stable_hash};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RenderId {
    String(String),
    Number(u32),
}

impl RenderId {
    pub(crate) fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            Self::Number(_) => None,
        }
    }

    pub(crate) fn as_number(&self) -> Option<u32> {
        match self {
            Self::Number(value) => Some(*value),
            Self::String(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NamedIdCandidate<K> {
    item: K,
    short_name: String,
    full_name: String,
}

impl<K> NamedIdCandidate<K> {
    fn new(item: K, short_name: String, full_name: String) -> Self {
        Self {
            item,
            short_name,
            full_name,
        }
    }
}

impl fmt::Display for RenderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(value) => formatter.write_str(value),
            Self::Number(value) => value.fmt(formatter),
        }
    }
}

pub(crate) fn assign_module_render_ids(
    options: &CompilerOptions,
    module_graph: &ModuleGraph,
    chunk_graph: &mut ChunkGraph,
) {
    let context = RenderPathContext::new(&options.context);
    let candidates = module_graph
        .modules()
        .iter()
        .filter(|module| !chunk_graph.module_chunks(module.handle()).is_empty())
        .map(|module| {
            let full_name = module_identity_key(&context, module.identity());
            let short_name = readable_module_name(&context, module.identity());
            NamedIdCandidate::new(module.handle(), short_name, full_name)
        })
        .collect();

    for (module, render_id) in assign_named_ids(candidates) {
        chunk_graph.set_module_render_id(module, render_id);
    }
}

pub(crate) fn assign_chunk_render_ids(
    options: &CompilerOptions,
    module_graph: &ModuleGraph,
    chunk_graph: &mut ChunkGraph,
) {
    let context = RenderPathContext::new(&options.context);
    let mut entry_candidates = Vec::new();
    let mut async_candidates = Vec::new();
    for chunk in chunk_graph.chunks() {
        if let Some(name) = chunk.name() {
            entry_candidates.push(NamedIdCandidate::new(
                chunk.handle(),
                name.to_string(),
                format!("entry:{name}"),
            ));
            continue;
        }

        let mut roots = chunk
            .root_modules()
            .iter()
            .filter_map(|module| module_graph.module(*module))
            .map(|module| {
                (
                    readable_module_name(&context, module.identity()),
                    module_identity_key(&context, module.identity()),
                )
            })
            .collect::<Vec<_>>();
        roots.sort();
        let short_name = roots
            .iter()
            .map(|(name, _)| request_to_id(name))
            .filter(|name| !name.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let full_name = format!(
            "async:{}",
            roots
                .iter()
                .map(|(_, key)| key.as_str())
                .collect::<Vec<_>>()
                .join("|")
        );
        async_candidates.push(NamedIdCandidate::new(chunk.handle(), short_name, full_name));
    }

    let mut assignments = assign_named_ids(entry_candidates);
    let reserved = assignments
        .iter()
        .map(|(_, render_id)| render_id.to_string())
        .collect();
    assignments.extend(assign_named_ids_with_reserved(async_candidates, reserved));
    for (chunk_handle, render_id) in assignments {
        chunk_graph.set_chunk_render_id(chunk_handle, render_id);
    }
}

fn assign_named_ids<K>(candidates: Vec<NamedIdCandidate<K>>) -> Vec<(K, RenderId)>
where
    K: Copy + Ord,
{
    assign_named_ids_with_reserved(candidates, HashSet::new())
}

fn assign_named_ids_with_reserved<K>(
    candidates: Vec<NamedIdCandidate<K>>,
    mut used: HashSet<String>,
) -> Vec<(K, RenderId)>
where
    K: Copy + Ord,
{
    let mut groups = BTreeMap::<String, Vec<(K, String)>>::new();
    for candidate in candidates {
        groups
            .entry(candidate.short_name)
            .or_default()
            .push((candidate.item, candidate.full_name));
    }

    let mut assigned = Vec::new();
    let mut unnamed = Vec::new();

    for (short_name, mut items) in groups {
        items.sort_by(|(left_item, left_name), (right_item, right_name)| {
            left_name.cmp(right_name).then(left_item.cmp(right_item))
        });
        if short_name.is_empty() {
            unnamed.extend(items);
            continue;
        }
        if items.len() == 1 && used.insert(short_name.clone()) {
            assigned.push((items[0].0, RenderId::String(short_name)));
            continue;
        }

        for (item, full_name) in items {
            let suffix = format!("{:06x}", stable_hash(&full_name) & 0xff_ffff);
            let base = format!("{short_name}-{suffix}");
            let mut name = base.clone();
            let mut index = 0_u32;
            while !used.insert(name.clone()) {
                name = format!("{base}-{index}");
                index += 1;
            }
            assigned.push((item, RenderId::String(name)));
        }
    }

    unnamed.sort_by(|(left_item, left_name), (right_item, right_name)| {
        left_name.cmp(right_name).then(left_item.cmp(right_item))
    });
    let mut next = 0_u32;
    for (item, _) in unnamed {
        while used.contains(&next.to_string()) {
            next += 1;
        }
        used.insert(next.to_string());
        assigned.push((item, RenderId::Number(next)));
        next += 1;
    }
    assigned.sort_by_key(|(item, _)| *item);
    assigned
}

fn readable_module_name(context: &RenderPathContext, identity: &crate::ModuleIdentity) -> String {
    let resource = context.make_relative(&identity.resource);
    if resource.is_empty() {
        return String::new();
    }
    let mut name = String::new();
    for loader in &identity.loaders {
        name.push_str(&context.make_request_relative(loader));
        name.push('!');
    }
    if !resource.starts_with('.') && !resource.starts_with('/') {
        name.push_str("./");
    }
    name.push_str(&resource);
    if let Some(query) = &identity.query {
        name.push('?');
        name.push_str(query.trim_start_matches('?'));
    }
    if let Some(fragment) = &identity.fragment {
        name.push('#');
        name.push_str(fragment.trim_start_matches('#'));
    }
    if let Some(layer) = &identity.layer {
        name.push_str("|layer:");
        name.push_str(layer);
    }
    name
}

fn module_identity_key(context: &RenderPathContext, identity: &crate::ModuleIdentity) -> String {
    format!(
        "type={:?};loaders={:?};resource={};query={:?};fragment={:?};layer={:?}",
        identity.module_type,
        identity.loaders,
        context.make_relative(&identity.resource),
        identity.query,
        identity.fragment,
        identity.layer
    )
}

fn request_to_id(request: &str) -> String {
    request
        .trim_start_matches("./")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
struct RenderPathContext {
    raw_context: PathBuf,
    context: PathBuf,
}

impl RenderPathContext {
    fn new(context: &Path) -> Self {
        Self {
            raw_context: context.to_path_buf(),
            context: std::fs::canonicalize(context).unwrap_or_else(|_| context.to_path_buf()),
        }
    }

    fn make_relative(&self, resource: &Path) -> String {
        if let Ok(relative) = resource
            .strip_prefix(&self.context)
            .or_else(|_| resource.strip_prefix(&self.raw_context))
        {
            return normalize_path(relative);
        }

        let resource = std::fs::canonicalize(resource).unwrap_or_else(|_| resource.to_path_buf());
        let relative = resource.strip_prefix(&self.context).unwrap_or(&resource);
        normalize_path(relative)
    }

    fn make_request_relative(&self, request: &str) -> String {
        let path = Path::new(request);
        if path.is_absolute() {
            self.make_relative(path)
        } else {
            request.replace('\\', "/")
        }
    }
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CacheOptions, CompilerOptions, Entry, ModuleGraph, ModuleIdentity, SnapshotOptions,
        cache::Cache,
        code_generation::{create_render_manifest, generate_code, render_assets},
    };

    #[test]
    fn named_ids_are_stable_across_insertion_order_and_collision_safe() {
        let first = assign_named_ids(vec![
            NamedIdCandidate::new(0, "same".to_string(), "alpha".to_string()),
            NamedIdCandidate::new(1, "same".to_string(), "beta".to_string()),
        ]);
        let second = assign_named_ids(vec![
            NamedIdCandidate::new(0, "same".to_string(), "beta".to_string()),
            NamedIdCandidate::new(1, "same".to_string(), "alpha".to_string()),
        ]);

        let first_by_identity =
            BTreeMap::from([("alpha", first[0].1.clone()), ("beta", first[1].1.clone())]);
        let second_by_identity = BTreeMap::from([
            ("beta", second[0].1.clone()),
            ("alpha", second[1].1.clone()),
        ]);

        assert_eq!(first_by_identity, second_by_identity);
        assert_ne!(first[0].1, first[1].1);
    }

    #[test]
    // Derived from webpack 5.108.1:
    // lib/ids/IdHelpers.js assignAscendingModuleIds/assignAscendingChunkIds.
    fn unnamed_items_receive_deterministic_numeric_fallbacks() {
        let assignments = assign_named_ids(vec![
            NamedIdCandidate::new(1, String::new(), "beta".to_string()),
            NamedIdCandidate::new(0, String::new(), "alpha".to_string()),
        ]);

        assert_eq!(
            assignments,
            vec![(0, RenderId::Number(0)), (1, RenderId::Number(1))]
        );
    }

    #[test]
    fn module_names_distinguish_loaders_queries_fragments_and_layers() {
        let context = RenderPathContext::new(Path::new("/project"));
        let mut identity = crate::ModuleIdentity::new("/project/src/value.js");
        identity.loaders = vec!["/project/loaders/first.js".to_string()];
        identity.query = Some("?one".to_string());
        identity.fragment = Some("#alpha".to_string());
        identity.layer = Some("client".to_string());

        assert_eq!(
            readable_module_name(&context, &identity),
            "loaders/first.js!./src/value.js?one#alpha|layer:client"
        );
    }

    #[test]
    fn emitted_assets_are_stable_across_module_graph_insertion_orders() {
        let first = render_synthetic_entries(&["index", "alpha", "omega"]);
        let second = render_synthetic_entries(&["omega", "index", "alpha"]);

        assert_eq!(first, second);
    }

    #[test]
    // Derived from webpack 5.108.1:
    // lib/ids/IdHelpers.js assignAscendingModuleIds fallback for unnamed modules.
    fn emitted_unnamed_module_consumes_numeric_fallback() {
        let context = Path::new("/project");
        let options = CompilerOptions::new(context, vec![Entry::new("main", "./index")]);
        let mut module_graph = ModuleGraph::default();
        let module = add_built_module(&mut module_graph, ModuleIdentity::new(context), "unnamed");
        let mut chunk_graph =
            crate::build_chunk_graph::build_chunk_graph(&options, &module_graph, &[module]);
        assign_module_render_ids(&options, &module_graph, &mut chunk_graph);
        assign_chunk_render_ids(&options, &module_graph, &mut chunk_graph);

        let cache = Cache::new(CacheOptions::memory(), SnapshotOptions::default());
        let results = generate_code(&module_graph, &chunk_graph).results;
        chunk_graph.process_runtime_requirements(
            results
                .runtime_requirements()
                .map(|(module, requirements)| (module, *requirements)),
        );
        let manifest = create_render_manifest(&chunk_graph, &[module], &results);
        let assets = render_assets(&options, &cache, &manifest, &results);
        let main = assets
            .iter()
            .find(|asset| asset.filename == "main.js")
            .expect("main asset should exist");

        assert!(main.source.contains("0: "));
        assert!(
            main.source
                .contains("module.exports = __webpack_require__(0);")
        );
    }

    fn render_synthetic_entries(insertion_order: &[&str]) -> Vec<crate::Asset> {
        let context = Path::new("/project");
        let entry_names = ["index", "alpha", "omega"];
        let options = CompilerOptions::new(
            context,
            entry_names
                .iter()
                .map(|name| Entry::new(*name, format!("./{name}")))
                .collect(),
        );
        let mut module_graph = ModuleGraph::default();
        let mut modules_by_name = BTreeMap::new();
        for name in insertion_order {
            let identity = ModuleIdentity::new(context.join(format!("{name}.js")));
            let module = add_built_module(&mut module_graph, identity, name);
            modules_by_name.insert(*name, module);
        }
        let entries = entry_names
            .iter()
            .map(|name| modules_by_name[name])
            .collect::<Vec<_>>();
        let mut chunk_graph =
            crate::build_chunk_graph::build_chunk_graph(&options, &module_graph, &entries);
        assign_module_render_ids(&options, &module_graph, &mut chunk_graph);
        assign_chunk_render_ids(&options, &module_graph, &mut chunk_graph);

        let cache = Cache::new(CacheOptions::memory(), SnapshotOptions::default());
        let results = generate_code(&module_graph, &chunk_graph).results;
        chunk_graph.process_runtime_requirements(
            results
                .runtime_requirements()
                .map(|(module, requirements)| (module, *requirements)),
        );
        let manifest = create_render_manifest(&chunk_graph, &entries, &results);
        render_assets(&options, &cache, &manifest, &results)
    }

    fn add_built_module(
        module_graph: &mut ModuleGraph,
        identity: ModuleIdentity,
        value: &str,
    ) -> crate::ModuleHandle {
        let module = module_graph.add_module(identity);
        let source = format!("const value = {value:?};");
        module_graph
            .module_mut(module)
            .expect("synthetic module should exist")
            .finish_build(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                source.clone(),
                stable_hash(&source),
            );
        module
    }
}
