// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/HarmonyImportSideEffectDependency.js

use serde::{Deserialize, Serialize};

use super::ModuleDependency;
use crate::{
    SourceRange,
    dependency_template::{
        DependencyTemplate, DependencyTemplateContext, import_var, json_render_id,
    },
    init_fragment::InitFragmentStage,
    runtime::RuntimeRequirement,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HarmonyImportSideEffectDependency {
    pub module: ModuleDependency,
    pub import_var: Option<String>,
}

impl HarmonyImportSideEffectDependency {
    pub fn new(
        request: impl Into<String>,
        source_order: usize,
        range: Option<SourceRange>,
    ) -> Self {
        let mut module = ModuleDependency::new(request, Some(source_order));
        module.range = range;
        Self {
            module,
            import_var: None,
        }
    }
}

pub(crate) struct HarmonyImportSideEffectDependencyTemplate;

impl DependencyTemplate<HarmonyImportSideEffectDependency>
    for HarmonyImportSideEffectDependencyTemplate
{
    fn source_ranges(&self, dependency: &HarmonyImportSideEffectDependency) -> Vec<SourceRange> {
        dependency.module.range.into_iter().collect()
    }

    fn apply(
        &self,
        dependency: &HarmonyImportSideEffectDependency,
        _source: &mut rspack_sources::ReplaceSource,
        context: &mut DependencyTemplateContext<'_>,
    ) {
        context.add_runtime_requirement(RuntimeRequirement::Require);
        let dependency_index = context
            .dependency_index
            .expect("Harmony import must have a Dependency Index");
        let target = context
            .module_graph
            .module_for_dependency(context.module, None, dependency_index)
            .expect("Harmony import must have a Module Graph connection");
        let Some(target_render_id) = context.module_render_ids.get(&target) else {
            return;
        };
        let import_var = import_var(
            &dependency.module.request,
            dependency.module.source_order.unwrap_or(0),
        );
        let target_id = json_render_id(target_render_id);
        context.add_init_fragment(
            InitFragmentStage::Import,
            format!("/* harmony import */ var {import_var} = __webpack_require__({target_id});\n"),
        );
    }
}
