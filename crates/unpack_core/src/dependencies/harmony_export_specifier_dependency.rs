// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/HarmonyExportSpecifierDependency.js

use serde::{Deserialize, Serialize};

use crate::{
    dependency_template::{DependencyTemplate, DependencyTemplateContext, property_name},
    init_fragment::InitFragmentStage,
    runtime::RuntimeRequirement,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HarmonyExportSpecifierDependency {
    pub id: String,
    pub name: String,
}

impl HarmonyExportSpecifierDependency {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

pub(crate) struct HarmonyExportSpecifierDependencyTemplate;

impl DependencyTemplate<HarmonyExportSpecifierDependency>
    for HarmonyExportSpecifierDependencyTemplate
{
    fn apply(
        &self,
        dependency: &HarmonyExportSpecifierDependency,
        _source: &mut rspack_sources::ReplaceSource,
        context: &mut DependencyTemplateContext<'_>,
    ) {
        context.add_runtime_requirement(RuntimeRequirement::DefinePropertyGetters);
        let Some(used_name) = context.exports_info.get_used_name(&dependency.name) else {
            return;
        };
        context.add_init_fragment(
            InitFragmentStage::Export,
            format!(
                "__webpack_require__.d(__webpack_exports__, {{ {}: () => ({}) }});\n",
                property_name(used_name),
                dependency.id
            ),
        );
    }
}
