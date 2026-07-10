use std::collections::{BTreeMap, BTreeSet};

use crate::{ChunkGraph, ChunkId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum RuntimeRequirement {
    ModuleFactories,
    ModuleCache,
    Require,
    DefinePropertyGetters,
    HasOwnProperty,
    MakeNamespaceObject,
    EnsureChunk,
    GetChunkFilename,
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
    HasOwnProperty,
    MakeNamespaceObject,
}

pub(crate) struct RuntimeModuleContext<'a> {
    pub(crate) chunk_graph: &'a ChunkGraph,
    pub(crate) runtime_chunk: ChunkId,
}

impl RuntimeModule {
    pub(crate) fn identifier(self) -> &'static str {
        match self {
            Self::DefinePropertyGetters => "webpack/runtime/define property getters",
            Self::HasOwnProperty => "webpack/runtime/hasOwnProperty shorthand",
            Self::MakeNamespaceObject => "webpack/runtime/make namespace object",
        }
    }

    pub(crate) fn stage(self) -> RuntimeModuleStage {
        match self {
            Self::HasOwnProperty | Self::DefinePropertyGetters | Self::MakeNamespaceObject => {
                RuntimeModuleStage::Normal
            }
        }
    }

    fn prerequisites(self) -> &'static [RuntimeRequirement] {
        match self {
            Self::DefinePropertyGetters => &[RuntimeRequirement::HasOwnProperty],
            Self::HasOwnProperty | Self::MakeNamespaceObject => &[],
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
            RuntimeRequirement::ModuleFactories
            | RuntimeRequirement::ModuleCache
            | RuntimeRequirement::Require
            | RuntimeRequirement::EnsureChunk
            | RuntimeRequirement::GetChunkFilename
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
    #[should_panic(expected = "conflicting Runtime Modules")]
    fn resolver_rejects_conflicting_module_identifiers() {
        let mut modules = BTreeMap::from([(
            RuntimeModule::DefinePropertyGetters.identifier(),
            RuntimeModule::HasOwnProperty,
        )]);

        insert_runtime_module(&mut modules, RuntimeModule::DefinePropertyGetters);
    }
}
