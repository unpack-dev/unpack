use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use rspack_sources::{
    ConcatSource, MapOptions, ObjectPool, OriginalSource, RawStringSource, ReplaceSource, Source,
};

use crate::{
    AsyncBlockOrigin, Chunk, ChunkGraph, ChunkGroupKind, CompilerOptions, ConstDependency,
    Dependency, Error, ExportsInfo, HarmonyExportExpressionDependency,
    HarmonyExportHeaderDependency, HarmonyExportImportedSpecifierDependency,
    HarmonyExportSpecifierDependency, HarmonyImportSideEffectDependency,
    HarmonyImportSpecifierDependency, ImportDependency, Module, ModuleGraph, ModuleId, SourceRange,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub filename: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RuntimeRequirement {
    ModuleFactories,
    ModuleCache,
    Require,
    DefinePropertyGetters,
    HasOwnProperty,
    MakeNamespaceObject,
    EnsureChunk,
    GetChunkFilename,
    RequireChunkLoading,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RuntimeRequirements {
    requirements: BTreeSet<RuntimeRequirement>,
}

impl RuntimeRequirements {
    pub fn insert(&mut self, requirement: RuntimeRequirement) {
        self.requirements.insert(requirement);
    }

    pub fn contains(&self, requirement: RuntimeRequirement) -> bool {
        self.requirements.contains(&requirement)
    }

    pub fn iter(&self) -> impl Iterator<Item = RuntimeRequirement> + '_ {
        self.requirements.iter().copied()
    }

    fn for_initial_chunk(chunk_graph: &ChunkGraph, chunk: &Chunk) -> Self {
        let mut requirements = Self::default();
        requirements.insert(RuntimeRequirement::ModuleFactories);
        requirements.insert(RuntimeRequirement::ModuleCache);
        requirements.insert(RuntimeRequirement::Require);
        requirements.insert(RuntimeRequirement::DefinePropertyGetters);
        requirements.insert(RuntimeRequirement::HasOwnProperty);
        requirements.insert(RuntimeRequirement::MakeNamespaceObject);

        let has_async_child = chunk.groups().iter().any(|group_id| {
            !chunk_graph.chunk_groups()[group_id.index()]
                .children()
                .is_empty()
        });
        if has_async_child {
            requirements.insert(RuntimeRequirement::EnsureChunk);
            requirements.insert(RuntimeRequirement::GetChunkFilename);
            requirements.insert(RuntimeRequirement::RequireChunkLoading);
        }

        requirements
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InitFragment {
    stage: InitFragmentStage,
    order: usize,
    content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum InitFragmentStage {
    HarmonyImport,
    HarmonyExport,
    HarmonyStarReexport,
}

pub(crate) fn create_assets(
    options: &CompilerOptions,
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    entries: &[ModuleId],
) -> Vec<Asset> {
    let module_render_ids = module_render_ids(options.context.as_path(), module_graph);
    let mut assets = Vec::new();

    for (entry_index, group_id) in chunk_graph.entrypoints().iter().copied().enumerate() {
        let group = &chunk_graph.chunk_groups()[group_id.index()];
        let Some(chunk_id) = group.chunks().first().copied() else {
            continue;
        };
        let Some(chunk) = chunk_graph.chunk(chunk_id) else {
            continue;
        };
        let Some(entry_module) = entries.get(entry_index).copied() else {
            continue;
        };
        assets.extend(emit_asset(
            chunk.filename().to_string(),
            render_initial_asset(
                options,
                module_graph,
                chunk_graph,
                chunk,
                entry_module,
                &module_render_ids,
            ),
        ));
    }

    for chunk in chunk_graph.chunks() {
        let is_initial = chunk.groups().iter().any(|group_id| {
            matches!(
                chunk_graph.chunk_groups()[group_id.index()].kind(),
                ChunkGroupKind::Entrypoint { .. }
            )
        });
        if is_initial {
            continue;
        }
        assets.extend(emit_asset(
            chunk.filename().to_string(),
            render_async_chunk_asset(module_graph, chunk_graph, chunk, &module_render_ids),
        ));
    }

    assets
}

fn emit_asset(filename: String, mut source: ConcatSource) -> Vec<Asset> {
    let map_filename = format!("{filename}.map");
    source.add(RawStringSource::from(format!(
        "\n//# sourceMappingURL={map_filename}\n"
    )));

    let mut assets = Vec::new();
    let mut source_map = source.map(&ObjectPool::default(), &MapOptions::default());
    if let Some(map) = &mut source_map {
        map.set_file(Some(filename.clone().into()));
    }

    assets.push(Asset {
        filename,
        source: source.source().into_string_lossy().into_owned(),
    });

    if let Some(map) = source_map {
        assets.push(Asset {
            filename: map_filename,
            source: map.to_json(),
        });
    }

    assets
}

fn render_initial_asset(
    _options: &CompilerOptions,
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    chunk: &Chunk,
    entry_module: ModuleId,
    module_render_ids: &HashMap<ModuleId, String>,
) -> ConcatSource {
    let modules = render_module_table(module_graph, chunk_graph, chunk, module_render_ids);
    let _runtime_requirements = RuntimeRequirements::for_initial_chunk(chunk_graph, chunk);
    let chunk_filename_map = render_chunk_filename_map(chunk_graph);
    let entry_id = json_string(&module_render_ids[&entry_module]);
    let chunk_id = json_string(chunk.render_id());

    let mut source = ConcatSource::default();
    source.add(RawStringSource::from(
        r#""use strict";
var __webpack_modules__ = ({
"#
        .to_string(),
    ));
    source.add(modules);
    source.add(RawStringSource::from(format!(
        r#"
}});

var __webpack_module_cache__ = {{}};
function __webpack_require__(moduleId) {{
  var cachedModule = __webpack_module_cache__[moduleId];
  if (cachedModule !== undefined) {{
    return cachedModule.exports;
  }}
  var module = __webpack_module_cache__[moduleId] = {{
    exports: {{}}
  }};
  __webpack_modules__[moduleId](module, module.exports, __webpack_require__);
  return module.exports;
}}
__webpack_require__.m = __webpack_modules__;
__webpack_require__.d = function(exports, definition) {{
  for(var key in definition) {{
    if(__webpack_require__.o(definition, key) && !__webpack_require__.o(exports, key)) {{
      Object.defineProperty(exports, key, {{ enumerable: true, get: definition[key] }});
    }}
  }}
}};
__webpack_require__.o = function(obj, prop) {{ return Object.prototype.hasOwnProperty.call(obj, prop); }};
__webpack_require__.r = function(exports) {{
  if(typeof Symbol !== "undefined" && Symbol.toStringTag) {{
    Object.defineProperty(exports, Symbol.toStringTag, {{ value: "Module" }});
  }}
  Object.defineProperty(exports, "__esModule", {{ value: true }});
}};
__webpack_require__.u = function(chunkId) {{
  return ({{{chunk_filename_map}}})[chunkId];
}};
__webpack_require__.f = {{}};
__webpack_require__.e = function(chunkId) {{
  return Promise.all(Object.keys(__webpack_require__.f).reduce(function(promises, key) {{
    __webpack_require__.f[key](chunkId, promises);
    return promises;
  }}, []));
}};
var installedChunks = {{
  {chunk_id}: 1
}};
var installChunk = function(chunk) {{
  var moreModules = chunk.modules, chunkIds = chunk.ids, runtime = chunk.runtime;
  for(var moduleId in moreModules) {{
    if(__webpack_require__.o(moreModules, moduleId)) {{
      __webpack_require__.m[moduleId] = moreModules[moduleId];
    }}
  }}
  if(runtime) runtime(__webpack_require__);
  for(var i = 0; i < chunkIds.length; i++) {{
    installedChunks[chunkIds[i]] = 1;
  }}
}};
__webpack_require__.f.require = function(chunkId, promises) {{
  if(!installedChunks[chunkId]) {{
    var installedChunk = require("./" + __webpack_require__.u(chunkId));
    if(!installedChunks[chunkId]) {{
      installChunk(installedChunk);
    }}
  }}
}};
module.exports = __webpack_require__({entry_id});
"#,
    )));
    source
}

fn render_async_chunk_asset(
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    chunk: &Chunk,
    module_render_ids: &HashMap<ModuleId, String>,
) -> ConcatSource {
    let modules = render_module_table(module_graph, chunk_graph, chunk, module_render_ids);
    let chunk_id = json_string(chunk.render_id());
    let mut source = ConcatSource::default();
    source.add(RawStringSource::from(format!(
        r#""use strict";
exports.id = {chunk_id};
exports.ids = [{chunk_id}];
exports.modules = ({{
"#
    )));
    source.add(modules);
    source.add(RawStringSource::from("\n});\n".to_string()));
    source
}

fn render_module_table(
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    chunk: &Chunk,
    module_render_ids: &HashMap<ModuleId, String>,
) -> ConcatSource {
    let mut source = ConcatSource::default();
    let mut first = true;
    for module_id in chunk_graph.chunk_modules(chunk.id()) {
        let Some(module) = module_graph.module(*module_id) else {
            continue;
        };
        if first {
            first = false;
        } else {
            source.add(RawStringSource::from(",\n".to_string()));
        }
        let render_id = &module_render_ids[module_id];
        let factory = render_module_factory(module_graph, chunk_graph, module, module_render_ids);
        source.add(RawStringSource::from(format!(
            "{}: ",
            json_string(render_id)
        )));
        source.add(factory);
    }
    source
}

fn render_module_factory(
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    module: &Module,
    module_render_ids: &HashMap<ModuleId, String>,
) -> ConcatSource {
    if let Some(error) = module.build_error() {
        return render_failed_module_factory(error);
    }

    let module_id = module.id();
    let module_render_id = &module_render_ids[&module_id];
    let mut source = ReplaceSource::new(OriginalSource::new(
        module.source().to_string(),
        module_render_id.to_string(),
    ));
    let mut init_fragments = Vec::new();

    for dependency in module.presentational_dependencies() {
        apply_dependency_template(
            dependency,
            module_id,
            None,
            module_graph,
            chunk_graph,
            module.exports_info(),
            module_render_ids,
            &mut source,
            &mut init_fragments,
        );
    }
    for dependency in module.dependencies() {
        apply_dependency_template(
            dependency,
            module_id,
            None,
            module_graph,
            chunk_graph,
            module.exports_info(),
            module_render_ids,
            &mut source,
            &mut init_fragments,
        );
    }
    for (block_index, block) in module.blocks().iter().enumerate() {
        for dependency in block.dependencies() {
            apply_dependency_template(
                dependency,
                module_id,
                Some(block_index),
                module_graph,
                chunk_graph,
                module.exports_info(),
                module_render_ids,
                &mut source,
                &mut init_fragments,
            );
        }
    }

    let init = render_init_fragments(init_fragments);
    let mut factory = ConcatSource::default();
    factory.add(RawStringSource::from(format!(
        "((__unused_webpack_module, __webpack_exports__, __webpack_require__) => {{\n\"use strict\";\n__webpack_require__.r(__webpack_exports__);\n{init}"
    )));
    factory.add(source);
    factory.add(RawStringSource::from("\n})".to_string()));
    factory
}

fn render_failed_module_factory(error: &Error) -> ConcatSource {
    let mut factory = ConcatSource::default();
    factory.add(RawStringSource::from(format!(
        "((__unused_webpack_module, __webpack_exports__, __webpack_require__) => {{\n\"use strict\";\nthrow new Error({});\n}})",
        json_string(&error.to_string())
    )));
    factory
}

fn render_init_fragments(mut fragments: Vec<InitFragment>) -> String {
    fragments.sort_by_key(|fragment| (fragment.stage, fragment.order));
    fragments
        .into_iter()
        .map(|fragment| fragment.content)
        .collect()
}

fn push_init_fragment(
    init_fragments: &mut Vec<InitFragment>,
    stage: InitFragmentStage,
    content: String,
) {
    init_fragments.push(InitFragment {
        stage,
        order: init_fragments.len(),
        content,
    });
}

#[allow(clippy::too_many_arguments)]
fn apply_dependency_template(
    dependency: &Dependency,
    module_id: ModuleId,
    origin_block: Option<usize>,
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    exports_info: &ExportsInfo,
    module_render_ids: &HashMap<ModuleId, String>,
    source: &mut ReplaceSource,
    init_fragments: &mut Vec<InitFragment>,
) {
    match dependency {
        Dependency::Const(dep) => apply_const_dependency(dep, source),
        Dependency::Null(_) => {}
        Dependency::HarmonyExportHeader(dep) => apply_export_header_dependency(dep, source),
        Dependency::HarmonyImportSideEffect(dep) => apply_harmony_import_side_effect_dependency(
            dep,
            module_id,
            module_graph,
            module_render_ids,
            init_fragments,
        ),
        Dependency::HarmonyImportSpecifier(dep) => apply_harmony_import_specifier_dependency(
            dep,
            module_id,
            module_graph,
            module_render_ids,
            source,
        ),
        Dependency::HarmonyExportSpecifier(dep) => {
            apply_harmony_export_specifier_dependency(dep, exports_info, init_fragments)
        }
        Dependency::HarmonyExportExpression(dep) => {
            apply_harmony_export_expression_dependency(dep, exports_info, source, init_fragments)
        }
        Dependency::HarmonyExportImportedSpecifier(dep) => {
            apply_harmony_export_imported_specifier_dependency(
                dep,
                module_id,
                module_graph,
                exports_info,
                module_render_ids,
                init_fragments,
            )
        }
        Dependency::Import(dep) => apply_import_dependency(
            dep,
            module_id,
            origin_block,
            module_graph,
            chunk_graph,
            module_render_ids,
            source,
        ),
        Dependency::Entry(_) => {}
    }
}

fn apply_const_dependency(dep: &ConstDependency, source: &mut ReplaceSource) {
    replace(source, dep.range, dep.expression.clone());
}

fn apply_export_header_dependency(dep: &HarmonyExportHeaderDependency, source: &mut ReplaceSource) {
    let end = dep
        .declaration_range
        .map(|range| range.start)
        .unwrap_or(dep.statement_range.end);
    replace(
        source,
        SourceRange::new(dep.statement_range.start, end),
        String::new(),
    );
}

fn apply_harmony_import_side_effect_dependency(
    dep: &HarmonyImportSideEffectDependency,
    module_id: ModuleId,
    module_graph: &ModuleGraph,
    module_render_ids: &HashMap<ModuleId, String>,
    init_fragments: &mut Vec<InitFragment>,
) {
    let Some(target) = module_graph.module_for_dependency(
        module_id,
        None,
        &Dependency::HarmonyImportSideEffect(dep.clone()),
    ) else {
        return;
    };
    let import_var = import_var(&dep.module.request, dep.module.source_order.unwrap_or(0));
    let target_id = json_string(&module_render_ids[&target]);
    push_init_fragment(
        init_fragments,
        InitFragmentStage::HarmonyImport,
        format!("/* harmony import */ var {import_var} = __webpack_require__({target_id});\n"),
    );
}

fn apply_harmony_import_specifier_dependency(
    dep: &HarmonyImportSpecifierDependency,
    module_id: ModuleId,
    module_graph: &ModuleGraph,
    module_render_ids: &HashMap<ModuleId, String>,
    source: &mut ReplaceSource,
) {
    let Some(_target) = module_graph.module_for_dependency(
        module_id,
        None,
        &Dependency::HarmonyImportSpecifier(dep.clone()),
    ) else {
        return;
    };
    let expression = import_expression(
        &dep.module.request,
        dep.module.source_order.unwrap_or(0),
        &dep.ids,
    );
    let expression = if dep.shorthand {
        format!("{}: {expression}", dep.name)
    } else {
        expression
    };
    replace(source, dep.usage_range, expression);
    let _ = module_render_ids;
}

fn apply_harmony_export_specifier_dependency(
    dep: &HarmonyExportSpecifierDependency,
    exports_info: &ExportsInfo,
    init_fragments: &mut Vec<InitFragment>,
) {
    let used_name = exports_info.get_used_name(&dep.name).unwrap_or(&dep.name);
    push_init_fragment(
        init_fragments,
        InitFragmentStage::HarmonyExport,
        format!(
            "__webpack_require__.d(__webpack_exports__, {{ {}: () => ({}) }});\n",
            property_name(used_name),
            dep.id
        ),
    );
}

fn apply_harmony_export_expression_dependency(
    dep: &HarmonyExportExpressionDependency,
    exports_info: &ExportsInfo,
    source: &mut ReplaceSource,
    init_fragments: &mut Vec<InitFragment>,
) {
    let binding = dep
        .declaration_id
        .clone()
        .unwrap_or_else(|| "__WEBPACK_DEFAULT_EXPORT__".to_string());
    if dep.declaration_id.is_some() {
        replace(
            source,
            SourceRange::new(dep.statement_range.start, dep.range.start),
            "/* harmony default export */ ".to_string(),
        );
    } else {
        replace(
            source,
            SourceRange::new(dep.statement_range.start, dep.range.start),
            "/* harmony default export */ const __WEBPACK_DEFAULT_EXPORT__ = ".to_string(),
        );
    }
    let used_name = exports_info.get_used_name("default").unwrap_or("default");
    push_init_fragment(
        init_fragments,
        InitFragmentStage::HarmonyExport,
        format!(
            "__webpack_require__.d(__webpack_exports__, {{ {}: () => ({binding}) }});\n",
            property_name(used_name)
        ),
    );
}

fn apply_harmony_export_imported_specifier_dependency(
    dep: &HarmonyExportImportedSpecifierDependency,
    module_id: ModuleId,
    module_graph: &ModuleGraph,
    exports_info: &ExportsInfo,
    module_render_ids: &HashMap<ModuleId, String>,
    init_fragments: &mut Vec<InitFragment>,
) {
    let dependency = Dependency::HarmonyExportImportedSpecifier(dep.clone());
    let Some(_target) = module_graph.module_for_dependency(module_id, None, &dependency) else {
        return;
    };
    let import_var = import_var(&dep.module.request, dep.module.source_order.unwrap_or(0));
    if dep.is_star {
        push_init_fragment(
            init_fragments,
            InitFragmentStage::HarmonyStarReexport,
            format!(
                "/* harmony reexport (unknown) */ for(const __WEBPACK_IMPORT_KEY__ in {import_var}) if(__WEBPACK_IMPORT_KEY__ !== \"default\" && __WEBPACK_IMPORT_KEY__ !== \"__esModule\") __webpack_require__.d(__webpack_exports__, {{ [__WEBPACK_IMPORT_KEY__]: () => ({import_var}[__WEBPACK_IMPORT_KEY__]) }});\n"
            ),
        );
    } else if let Some(name) = &dep.name {
        let expression = export_access_expression(&import_var, &dep.ids);
        let used_name = exports_info.get_used_name(name).unwrap_or(name);
        push_init_fragment(
            init_fragments,
            InitFragmentStage::HarmonyExport,
            format!(
                "__webpack_require__.d(__webpack_exports__, {{ {}: () => ({expression}) }});\n",
                property_name(used_name),
            ),
        );
    }
    let _ = module_render_ids;
}

fn apply_import_dependency(
    dep: &ImportDependency,
    module_id: ModuleId,
    origin_block: Option<usize>,
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
    module_render_ids: &HashMap<ModuleId, String>,
    source: &mut ReplaceSource,
) {
    let Some(block_index) = origin_block else {
        return;
    };
    let dependency = Dependency::Import(dep.clone());
    let Some(target) =
        module_graph.module_for_dependency(module_id, Some(block_index), &dependency)
    else {
        return;
    };
    let target_id = json_string(&module_render_ids[&target]);
    let origin = AsyncBlockOrigin {
        module: module_id,
        block_index,
    };
    let expression = if let Some(group_id) = chunk_graph.block_chunk_group(origin) {
        let group = &chunk_graph.chunk_groups()[group_id.index()];
        let chunk_id = group
            .chunks()
            .first()
            .and_then(|chunk_id| chunk_graph.chunk(*chunk_id))
            .map(|chunk| json_string(chunk.render_id()))
            .unwrap_or_else(|| "\"\"".to_string());
        format!(
            "__webpack_require__.e({chunk_id}).then(__webpack_require__.bind(__webpack_require__, {target_id}))"
        )
    } else {
        format!(
            "Promise.resolve().then(__webpack_require__.bind(__webpack_require__, {target_id}))"
        )
    };
    replace(source, dep.range(), expression);
}

fn replace(source: &mut ReplaceSource, range: SourceRange, content: String) {
    source.replace(range.start, range.end, content, None);
}

fn module_render_ids(context: &Path, module_graph: &ModuleGraph) -> HashMap<ModuleId, String> {
    module_graph
        .modules()
        .iter()
        .map(|module| (module.id(), module_render_id(context, module)))
        .collect()
}

fn module_render_id(context: &Path, module: &Module) -> String {
    let mut resource = make_relative(context, &module.identity().resource);
    if !resource.starts_with("./") {
        resource = format!("./{resource}");
    }
    if let Some(query) = &module.identity().query {
        resource.push('?');
        resource.push_str(query);
    }
    if let Some(fragment) = &module.identity().fragment {
        resource.push('#');
        resource.push_str(fragment);
    }
    resource
}

fn make_relative(context: &Path, resource: &Path) -> String {
    let context = std::fs::canonicalize(context).unwrap_or_else(|_| context.to_path_buf());
    let resource = std::fs::canonicalize(resource).unwrap_or_else(|_| PathBuf::from(resource));
    let relative = resource.strip_prefix(&context).unwrap_or(&resource);
    relative.to_string_lossy().replace('\\', "/")
}

fn render_chunk_filename_map(chunk_graph: &ChunkGraph) -> String {
    let mut entries = BTreeMap::new();
    for chunk in chunk_graph.chunks() {
        entries.insert(chunk.render_id().to_string(), chunk.filename().to_string());
    }
    entries
        .into_iter()
        .map(|(chunk_id, filename)| {
            format!("{}: {}", json_string(&chunk_id), json_string(&filename))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn import_var(request: &str, source_order: usize) -> String {
    let ident = sanitize_identifier(request);
    let index = source_order.saturating_sub(1);
    format!("_{ident}__WEBPACK_IMPORTED_MODULE_{index}__")
}

fn import_expression(request: &str, source_order: usize, ids: &[String]) -> String {
    let import_var = import_var(request, source_order);
    export_access_expression(&import_var, ids)
}

fn export_access_expression(base: &str, ids: &[String]) -> String {
    let mut expression = base.to_string();
    for id in ids {
        expression.push_str(&property_access(id));
    }
    expression
}

fn sanitize_identifier(value: &str) -> String {
    let mut ident = value
        .trim_start_matches("./")
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    if ident.is_empty() {
        ident.push_str("module");
    }
    if ident.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        ident.insert(0, '_');
    }
    ident
}

fn property_access(property: &str) -> String {
    if is_identifier(property) {
        format!(".{property}")
    } else {
        format!("[{}]", json_string(property))
    }
}

fn property_name(property: &str) -> String {
    if is_identifier(property) {
        property.to_string()
    } else {
        format!("[{}]", json_string(property))
    }
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first == '$' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

fn json_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("\"{escaped}\"")
}
