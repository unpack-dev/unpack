use std::collections::BTreeMap;

use crate::{
    ChunkGraph, ChunkHandle,
    id_assignment::{ChunkId, IdValue},
    output_filename::resolve_chunk_filename,
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

const ALL_RUNTIME_REQUIREMENTS: [RuntimeRequirement; 11] = [
    RuntimeRequirement::ModuleFactories,
    RuntimeRequirement::ModuleCache,
    RuntimeRequirement::Require,
    RuntimeRequirement::DefinePropertyGetters,
    RuntimeRequirement::HasOwnProperty,
    RuntimeRequirement::MakeNamespaceObject,
    RuntimeRequirement::EnsureChunk,
    RuntimeRequirement::EnsureChunkHandlers,
    RuntimeRequirement::GetChunkFilename,
    RuntimeRequirement::ModuleFactoriesAddOnly,
    RuntimeRequirement::ReturnExportsFromRuntime,
];

const RUNTIME_REQUIREMENTS_MASK: u16 = {
    let mut mask = 0;
    let mut index = 0;
    while index < ALL_RUNTIME_REQUIREMENTS.len() {
        mask |= ALL_RUNTIME_REQUIREMENTS[index].mask();
        index += 1;
    }
    mask
};

impl RuntimeRequirement {
    const fn bit(self) -> u16 {
        // These bit positions are persisted in PackFile records. Keep existing
        // positions stable when adding or reordering the enum variants.
        match self {
            Self::ModuleFactories => 0,
            Self::ModuleCache => 1,
            Self::Require => 2,
            Self::DefinePropertyGetters => 3,
            Self::HasOwnProperty => 4,
            Self::MakeNamespaceObject => 5,
            Self::EnsureChunk => 6,
            Self::GetChunkFilename => 7,
            Self::ReturnExportsFromRuntime => 8,
            Self::EnsureChunkHandlers => 9,
            Self::ModuleFactoriesAddOnly => 10,
        }
    }

    const fn mask(self) -> u16 {
        1 << self.bit()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeRequirements {
    bits: u16,
}

impl RuntimeRequirements {
    pub(crate) fn all() -> impl Iterator<Item = RuntimeRequirement> {
        ALL_RUNTIME_REQUIREMENTS.into_iter()
    }

    pub(crate) const fn valid_mask() -> u16 {
        RUNTIME_REQUIREMENTS_MASK
    }

    pub(crate) fn insert(&mut self, requirement: RuntimeRequirement) -> bool {
        let mask = requirement.mask();
        let changed = self.bits & mask == 0;
        self.bits |= mask;
        changed
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, requirement: RuntimeRequirement) -> bool {
        self.bits & requirement.mask() != 0
    }

    pub(crate) fn extend(&mut self, other: &Self) {
        self.bits |= other.bits;
    }

    pub(crate) fn iter(self) -> impl Iterator<Item = RuntimeRequirement> {
        Self::all().filter(move |requirement| self.bits & requirement.mask() != 0)
    }

    pub(crate) const fn to_mask(self) -> u16 {
        self.bits
    }

    pub(crate) const fn from_mask(mask: u16) -> Option<Self> {
        if mask & !RUNTIME_REQUIREMENTS_MASK != 0 {
            return None;
        }
        Some(Self { bits: mask })
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

const ALL_RUNTIME_MODULES: [RuntimeModule; 7] = [
    RuntimeModule::DefinePropertyGetters,
    RuntimeModule::GetChunkFilename,
    RuntimeModule::HasOwnProperty,
    RuntimeModule::MakeNamespaceObject,
    RuntimeModule::ModuleFactoriesAddOnly,
    RuntimeModule::EnsureChunk,
    RuntimeModule::RequireChunkLoading,
];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct RuntimeModules(u8);

impl RuntimeModules {
    fn insert(&mut self, module: RuntimeModule) -> bool {
        let mask = module.mask();
        let changed = self.0 & mask == 0;
        self.0 |= mask;
        changed
    }

    fn iter(self) -> impl Iterator<Item = RuntimeModule> {
        ALL_RUNTIME_MODULES
            .into_iter()
            .filter(move |module| self.0 & module.mask() != 0)
    }
}

pub(crate) struct RuntimeModuleContext<'a> {
    pub(crate) chunk_graph: &'a ChunkGraph,
    pub(crate) runtime_chunk: ChunkHandle,
}

impl RuntimeModule {
    const fn mask(self) -> u8 {
        1 << match self {
            Self::DefinePropertyGetters => 0,
            Self::EnsureChunk => 1,
            Self::GetChunkFilename => 2,
            Self::HasOwnProperty => 3,
            Self::MakeNamespaceObject => 4,
            Self::ModuleFactoriesAddOnly => 5,
            Self::RequireChunkLoading => 6,
        }
    }

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
                let chunk_id = json_chunk_id(runtime_chunk.expect_id());
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
    let mut requirements = *direct;
    let mut modules = RuntimeModules::default();

    loop {
        let previous_requirements = requirements;
        for requirement in requirements.iter() {
            let Some(module) = runtime_module_for_requirement(requirement) else {
                continue;
            };
            modules.insert(module);
            for prerequisite in module.prerequisites() {
                requirements.insert(*prerequisite);
            }
        }
        if requirements == previous_requirements {
            break;
        }
    }

    let mut modules = modules.iter().collect::<Vec<_>>();
    modules.sort_by_key(|module| {
        let stage = RUNTIME_MODULE_STAGE_ORDER
            .iter()
            .position(|stage| *stage == module.stage())
            .expect("Runtime Module must use a known stage");
        (stage, module.identifier())
    });
    (requirements, modules)
}

fn runtime_module_for_requirement(requirement: RuntimeRequirement) -> Option<RuntimeModule> {
    match requirement {
        RuntimeRequirement::DefinePropertyGetters => Some(RuntimeModule::DefinePropertyGetters),
        RuntimeRequirement::HasOwnProperty => Some(RuntimeModule::HasOwnProperty),
        RuntimeRequirement::MakeNamespaceObject => Some(RuntimeModule::MakeNamespaceObject),
        RuntimeRequirement::EnsureChunk => Some(RuntimeModule::EnsureChunk),
        RuntimeRequirement::EnsureChunkHandlers => Some(RuntimeModule::RequireChunkLoading),
        RuntimeRequirement::GetChunkFilename => Some(RuntimeModule::GetChunkFilename),
        RuntimeRequirement::ModuleFactoriesAddOnly => Some(RuntimeModule::ModuleFactoriesAddOnly),
        RuntimeRequirement::ModuleFactories
        | RuntimeRequirement::ModuleCache
        | RuntimeRequirement::Require
        | RuntimeRequirement::ReturnExportsFromRuntime => None,
    }
}

fn render_chunk_filename_map(chunk_graph: &ChunkGraph) -> String {
    let mut entries = BTreeMap::new();
    for chunk in chunk_graph.chunks() {
        entries.insert(chunk.expect_id().clone(), resolve_chunk_filename(chunk));
    }
    entries
        .into_iter()
        .map(|(chunk_id, filename)| {
            format!(
                "{}: {}",
                json_chunk_id(&chunk_id),
                simd_json::to_string(&filename)
                    .expect("Chunk filename must serialize as a JavaScript string")
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn json_chunk_id(id: &ChunkId) -> String {
    match id.value() {
        IdValue::String(value) => {
            simd_json::to_string(value).expect("Chunk ID must serialize as a JavaScript string")
        }
        IdValue::Number(value) => value.to_string(),
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
    fn runtime_requirements_use_compact_inline_storage() {
        assert_eq!(
            std::mem::size_of::<RuntimeRequirements>(),
            std::mem::size_of::<u16>()
        );
    }

    #[test]
    fn runtime_requirement_masks_keep_pack_file_bit_positions_stable() {
        let persisted_bits = [
            (RuntimeRequirement::ModuleFactories, 0),
            (RuntimeRequirement::ModuleCache, 1),
            (RuntimeRequirement::Require, 2),
            (RuntimeRequirement::DefinePropertyGetters, 3),
            (RuntimeRequirement::HasOwnProperty, 4),
            (RuntimeRequirement::MakeNamespaceObject, 5),
            (RuntimeRequirement::EnsureChunk, 6),
            (RuntimeRequirement::GetChunkFilename, 7),
            (RuntimeRequirement::ReturnExportsFromRuntime, 8),
            (RuntimeRequirement::EnsureChunkHandlers, 9),
            (RuntimeRequirement::ModuleFactoriesAddOnly, 10),
        ];

        for (requirement, bit) in persisted_bits {
            let mut requirements = RuntimeRequirements::default();
            requirements.insert(requirement);
            assert_eq!(requirements.to_mask(), 1 << bit);
            assert_eq!(RuntimeRequirements::from_mask(1 << bit), Some(requirements));
        }
        assert_eq!(RuntimeRequirements::from_mask(1 << 15), None);
    }

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
    fn runtime_module_mask_is_compact_and_uses_stable_module_order() {
        assert_eq!(
            std::mem::size_of::<RuntimeModules>(),
            std::mem::size_of::<u8>()
        );

        let mut modules = RuntimeModules::default();
        for module in ALL_RUNTIME_MODULES {
            assert!(modules.insert(module));
            assert!(!modules.insert(module));
        }
        assert_eq!(modules.iter().collect::<Vec<_>>(), ALL_RUNTIME_MODULES);

        for (index, module) in ALL_RUNTIME_MODULES.iter().enumerate() {
            for other in &ALL_RUNTIME_MODULES[index + 1..] {
                assert_ne!(module.identifier(), other.identifier());
            }
        }
        assert!(ALL_RUNTIME_MODULES.windows(2).all(|modules| {
            (modules[0].stage(), modules[0].identifier())
                <= (modules[1].stage(), modules[1].identifier())
        }));
    }
}
