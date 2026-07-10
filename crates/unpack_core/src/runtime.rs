use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ChunkGraph, ChunkId, id_assignment::RenderId, output_filename::resolve_chunk_filename,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum RuntimeRequirement {
    ModuleFactories,
    ModuleCache,
    Require,
    DefinePropertyGetters,
    HasOwnProperty,
    MakeNamespaceObject,
    EnsureChunk,
    EnsureChunkHandlers,
    GetChunkFilename,
    ModuleFactoriesAddOnly,
    ReturnExportsFromRuntime,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeRequirements {
    requirements: BTreeSet<RuntimeRequirement>,
}

impl RuntimeRequirements {
    pub(crate) fn insert(&mut self, requirement: RuntimeRequirement) -> bool {
        self.requirements.insert(requirement)
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, requirement: RuntimeRequirement) -> bool {
        self.requirements.contains(&requirement)
    }

    pub(crate) fn extend(&mut self, other: &Self) {
        self.requirements.extend(other.iter());
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = RuntimeRequirement> + '_ {
        self.requirements.iter().copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum RuntimeModuleStage {
    Normal,
    Basic,
    Attach,
    Trigger,
}

const RUNTIME_MODULE_STAGE_ORDER: [RuntimeModuleStage; 4] = [
    RuntimeModuleStage::Normal,
    RuntimeModuleStage::Basic,
    RuntimeModuleStage::Attach,
    RuntimeModuleStage::Trigger,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RuntimeModule {
    DefinePropertyGetters,
    EnsureChunk,
    GetChunkFilename,
    HasOwnProperty,
    MakeNamespaceObject,
    ModuleFactoriesAddOnly,
    RequireChunkLoading,
}

pub(crate) struct RuntimeModuleContext<'a> {
    pub(crate) chunk_graph: &'a ChunkGraph,
    pub(crate) runtime_chunk: ChunkId,
}

impl RuntimeModule {
    pub(crate) fn identifier(self) -> &'static str {
        match self {
            Self::DefinePropertyGetters => "webpack/runtime/define property getters",
            Self::EnsureChunk => "webpack/runtime/ensure chunk",
            Self::GetChunkFilename => "webpack/runtime/get javascript chunk filename",
            Self::HasOwnProperty => "webpack/runtime/hasOwnProperty shorthand",
            Self::MakeNamespaceObject => "webpack/runtime/make namespace object",
            Self::ModuleFactoriesAddOnly => "webpack/runtime/module factories add only",
            Self::RequireChunkLoading => "webpack/runtime/require chunk loading",
        }
    }

    pub(crate) fn stage(self) -> RuntimeModuleStage {
        match self {
            Self::DefinePropertyGetters
            | Self::GetChunkFilename
            | Self::HasOwnProperty
            | Self::MakeNamespaceObject
            | Self::ModuleFactoriesAddOnly => RuntimeModuleStage::Normal,
            Self::EnsureChunk => RuntimeModuleStage::Basic,
            Self::RequireChunkLoading => RuntimeModuleStage::Attach,
        }
    }

    fn prerequisites(self) -> &'static [RuntimeRequirement] {
        match self {
            Self::DefinePropertyGetters => &[RuntimeRequirement::HasOwnProperty],
            Self::EnsureChunk => &[RuntimeRequirement::EnsureChunkHandlers],
            Self::RequireChunkLoading => &[
                RuntimeRequirement::GetChunkFilename,
                RuntimeRequirement::ModuleFactoriesAddOnly,
                RuntimeRequirement::HasOwnProperty,
            ],
            Self::GetChunkFilename
            | Self::HasOwnProperty
            | Self::MakeNamespaceObject
            | Self::ModuleFactoriesAddOnly => &[],
        }
    }

    pub(crate) fn generate(self, context: &RuntimeModuleContext<'_>) -> String {
        let _ = context
            .chunk_graph
            .chunk(context.runtime_chunk)
            .expect("Runtime Module context must reference an existing runtime Chunk");
        match self {
            Self::DefinePropertyGetters => r#"__webpack_require__.d = function(exports, definition) {
  for(var key in definition) {
    if(__webpack_require__.o(definition, key) && !__webpack_require__.o(exports, key)) {
      Object.defineProperty(exports, key, { enumerable: true, get: definition[key] });
    }
  }
};
"#
            .to_string(),
            Self::HasOwnProperty => {
                "__webpack_require__.o = function(obj, prop) { return Object.prototype.hasOwnProperty.call(obj, prop); };\n".to_string()
            }
            Self::MakeNamespaceObject => r#"__webpack_require__.r = function(exports) {
  if(typeof Symbol !== "undefined" && Symbol.toStringTag) {
    Object.defineProperty(exports, Symbol.toStringTag, { value: "Module" });
  }
  Object.defineProperty(exports, "__esModule", { value: true });
};
"#
            .to_string(),
            Self::EnsureChunk => r#"__webpack_require__.f = {};
__webpack_require__.e = function(chunkId) {
  return Promise.all(Object.keys(__webpack_require__.f).reduce(function(promises, key) {
    __webpack_require__.f[key](chunkId, promises);
    return promises;
  }, []));
};
"#
            .to_string(),
            Self::GetChunkFilename => {
                let filename_map = render_chunk_filename_map(context.chunk_graph);
                format!(
                    "__webpack_require__.u = function(chunkId) {{\n  return ({{{filename_map}}})[chunkId];\n}};\n"
                )
            }
            Self::ModuleFactoriesAddOnly => {
                "__webpack_require__.m = __webpack_modules__;\n".to_string()
            }
            Self::RequireChunkLoading => {
                let runtime_chunk = context
                    .chunk_graph
                    .chunk(context.runtime_chunk)
                    .expect("Require Chunk Loading must reference an existing runtime Chunk");
                let chunk_id = json_render_id(runtime_chunk.render_id());
                format!(
                    r#"var installedChunks = {{
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
"#
                )
            }
        }
    }
}

pub(crate) fn resolve_runtime_modules(
    direct: &RuntimeRequirements,
) -> (RuntimeRequirements, Vec<RuntimeModule>) {
    let mut requirements = direct.clone();
    let mut modules = BTreeMap::new();
    let mut pending = requirements.iter().collect::<Vec<_>>();

    while let Some(requirement) = pending.pop() {
        let module = match requirement {
            RuntimeRequirement::DefinePropertyGetters => Some(RuntimeModule::DefinePropertyGetters),
            RuntimeRequirement::HasOwnProperty => Some(RuntimeModule::HasOwnProperty),
            RuntimeRequirement::MakeNamespaceObject => Some(RuntimeModule::MakeNamespaceObject),
            RuntimeRequirement::EnsureChunk => Some(RuntimeModule::EnsureChunk),
            RuntimeRequirement::EnsureChunkHandlers => Some(RuntimeModule::RequireChunkLoading),
            RuntimeRequirement::GetChunkFilename => Some(RuntimeModule::GetChunkFilename),
            RuntimeRequirement::ModuleFactoriesAddOnly => {
                Some(RuntimeModule::ModuleFactoriesAddOnly)
            }
            RuntimeRequirement::ModuleFactories
            | RuntimeRequirement::ModuleCache
            | RuntimeRequirement::Require
            | RuntimeRequirement::ReturnExportsFromRuntime => None,
        };
        let Some(module) = module else {
            continue;
        };
        insert_runtime_module(&mut modules, module);
        for prerequisite in module.prerequisites() {
            if requirements.insert(*prerequisite) {
                pending.push(*prerequisite);
            }
        }
    }

    let mut modules = modules.into_values().collect::<Vec<_>>();
    modules.sort_by_key(|module| {
        let stage = RUNTIME_MODULE_STAGE_ORDER
            .iter()
            .position(|stage| *stage == module.stage())
            .expect("Runtime Module must use a known stage");
        (stage, module.identifier())
    });
    (requirements, modules)
}

fn render_chunk_filename_map(chunk_graph: &ChunkGraph) -> String {
    let mut entries = BTreeMap::new();
    for chunk in chunk_graph.chunks() {
        entries.insert(chunk.render_id().clone(), resolve_chunk_filename(chunk));
    }
    entries
        .into_iter()
        .map(|(chunk_id, filename)| {
            format!(
                "{}: {}",
                json_render_id(&chunk_id),
                simd_json::to_string(&filename)
                    .expect("Chunk filename must serialize as a JavaScript string")
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn json_render_id(render_id: &RenderId) -> String {
    match render_id {
        RenderId::String(value) => simd_json::to_string(value)
            .expect("Chunk Render ID must serialize as a JavaScript string"),
        RenderId::Number(value) => value.to_string(),
    }
}

fn insert_runtime_module(
    modules: &mut BTreeMap<&'static str, RuntimeModule>,
    module: RuntimeModule,
) {
    if let Some(existing) = modules.insert(module.identifier(), module) {
        assert_eq!(
            existing, module,
            "conflicting Runtime Modules must not share an identifier"
        );
    }
}

pub(crate) fn entry_startup_runtime_requirements() -> RuntimeRequirements {
    let mut requirements = RuntimeRequirements::default();
    requirements.insert(RuntimeRequirement::ModuleFactories);
    requirements.insert(RuntimeRequirement::ModuleCache);
    requirements.insert(RuntimeRequirement::Require);
    requirements.insert(RuntimeRequirement::ReturnExportsFromRuntime);
    requirements
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_closes_transitive_requirements_and_orders_modules() {
        let mut direct = RuntimeRequirements::default();
        direct.insert(RuntimeRequirement::MakeNamespaceObject);
        direct.insert(RuntimeRequirement::DefinePropertyGetters);

        let (closed, modules) = resolve_runtime_modules(&direct);

        assert!(closed.contains(RuntimeRequirement::HasOwnProperty));
        assert_eq!(
            modules,
            [
                RuntimeModule::DefinePropertyGetters,
                RuntimeModule::HasOwnProperty,
                RuntimeModule::MakeNamespaceObject,
            ]
        );
        assert!(
            modules
                .iter()
                .all(|module| module.stage() == RuntimeModuleStage::Normal)
        );
    }

    #[test]
    fn chunk_ensure_selects_the_complete_node_require_runtime_in_stage_order() {
        let mut direct = RuntimeRequirements::default();
        direct.insert(RuntimeRequirement::EnsureChunk);

        let (closed, modules) = resolve_runtime_modules(&direct);

        assert!(closed.contains(RuntimeRequirement::EnsureChunkHandlers));
        assert!(closed.contains(RuntimeRequirement::GetChunkFilename));
        assert!(closed.contains(RuntimeRequirement::ModuleFactoriesAddOnly));
        assert!(closed.contains(RuntimeRequirement::HasOwnProperty));
        assert_eq!(
            modules,
            [
                RuntimeModule::GetChunkFilename,
                RuntimeModule::HasOwnProperty,
                RuntimeModule::ModuleFactoriesAddOnly,
                RuntimeModule::EnsureChunk,
                RuntimeModule::RequireChunkLoading,
            ]
        );
        assert_eq!(modules[3].stage(), RuntimeModuleStage::Basic);
        assert_eq!(modules[4].stage(), RuntimeModuleStage::Attach);
    }

    #[test]
    #[should_panic(expected = "conflicting Runtime Modules")]
    fn resolver_rejects_conflicting_module_identifiers() {
        let mut modules = BTreeMap::from([(
            RuntimeModule::DefinePropertyGetters.identifier(),
            RuntimeModule::HasOwnProperty,
        )]);

        insert_runtime_module(&mut modules, RuntimeModule::DefinePropertyGetters);
    }
}
