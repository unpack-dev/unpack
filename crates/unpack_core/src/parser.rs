use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::{DependenciesBlock, Dependency, ModuleType};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ParsedModule {
    pub dependencies_block: DependenciesBlock,
    pub presentational_dependencies: Vec<Dependency>,
    pub data: ParsedModuleData,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ParsedModuleData {
    #[default]
    JavaScript,
    Json(serde_json::Value),
    Asset {
        module_type: ModuleType,
    },
}

impl Hash for ParsedModuleData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::JavaScript => 0_u8.hash(state),
            Self::Json(value) => {
                1_u8.hash(state);
                serde_json::to_string(value)
                    .expect("parsed JSON values must serialize")
                    .hash(state);
            }
            Self::Asset { module_type } => {
                2_u8.hash(state);
                module_type.hash(state);
            }
        }
    }
}
