// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/HarmonyExportExpressionDependency.js

use serde::{Deserialize, Serialize};

use crate::{
    SourceRange,
    dependency_template::{DependencyTemplate, DependencyTemplateContext, property_name, replace},
    init_fragment::InitFragmentStage,
    runtime::RuntimeRequirement,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HarmonyExportExpressionDependency {
    pub range: SourceRange,
    pub statement_range: SourceRange,
    pub declaration_id: Option<String>,
}

impl HarmonyExportExpressionDependency {
    pub fn new(
        range: SourceRange,
        statement_range: SourceRange,
        declaration_id: Option<String>,
    ) -> Self {
        Self {
            range,
            statement_range,
            declaration_id,
        }
    }
}

pub(crate) struct HarmonyExportExpressionDependencyTemplate;

impl DependencyTemplate<HarmonyExportExpressionDependency>
    for HarmonyExportExpressionDependencyTemplate
{
    fn source_ranges(&self, dependency: &HarmonyExportExpressionDependency) -> Vec<SourceRange> {
        vec![dependency.statement_range, dependency.range]
    }

    fn apply(
        &self,
        dependency: &HarmonyExportExpressionDependency,
        source: &mut rspack_sources::ReplaceSource,
        context: &mut DependencyTemplateContext<'_>,
    ) {
        context.add_runtime_requirement(RuntimeRequirement::DefinePropertyGetters);
        let binding = dependency
            .declaration_id
            .clone()
            .unwrap_or_else(|| "__WEBPACK_DEFAULT_EXPORT__".to_string());
        if dependency.declaration_id.is_some() {
            replace(
                source,
                SourceRange::new(dependency.statement_range.start, dependency.range.start),
                "/* harmony default export */ ".to_string(),
            );
        } else {
            replace(
                source,
                SourceRange::new(dependency.statement_range.start, dependency.range.start),
                "/* harmony default export */ const __WEBPACK_DEFAULT_EXPORT__ = ".to_string(),
            );
        }
        let Some(used_name) = context.exports_info.get_used_name("default") else {
            return;
        };
        context.add_init_fragment(
            InitFragmentStage::Export,
            format!(
                "__webpack_require__.d(__webpack_exports__, {{ {}: () => ({binding}) }});\n",
                property_name(used_name)
            ),
        );
    }
}
