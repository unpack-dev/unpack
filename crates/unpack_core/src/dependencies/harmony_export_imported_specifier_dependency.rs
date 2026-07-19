// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/HarmonyExportImportedSpecifierDependency.js

use serde::{Deserialize, Serialize};

use super::ModuleDependency;
use crate::{
    SourceRange,
    dependency_template::{
        DependencyTemplate, DependencyTemplateContext, export_access_expression, import_var,
        property_name,
    },
    init_fragment::InitFragmentStage,
    runtime::RuntimeRequirement,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HarmonyExportImportedSpecifierDependency {
    pub module: ModuleDependency,
    pub ids: Vec<String>,
    pub name: Option<String>,
    pub is_star: bool,
}

impl HarmonyExportImportedSpecifierDependency {
    pub fn new(
        request: impl Into<String>,
        source_order: usize,
        ids: Vec<String>,
        name: Option<String>,
        is_star: bool,
        range: Option<SourceRange>,
    ) -> Self {
        let mut module = ModuleDependency::new(request, Some(source_order));
        module.range = range;
        Self {
            module,
            ids,
            name,
            is_star,
        }
    }
}

pub(crate) struct HarmonyExportImportedSpecifierDependencyTemplate;

impl DependencyTemplate<HarmonyExportImportedSpecifierDependency>
    for HarmonyExportImportedSpecifierDependencyTemplate
{
    fn source_ranges(
        &self,
        dependency: &HarmonyExportImportedSpecifierDependency,
    ) -> Vec<SourceRange> {
        dependency.module.range.into_iter().collect()
    }

    fn apply(
        &self,
        dependency: &HarmonyExportImportedSpecifierDependency,
        _source: &mut rspack_sources::ReplaceSource,
        context: &mut DependencyTemplateContext<'_>,
    ) {
        context.add_runtime_requirement(RuntimeRequirement::DefinePropertyGetters);
        let dependency_index = context
            .dependency_index
            .expect("Harmony re-export must have a Dependency Index");
        let target = context
            .module_graph
            .module_for_dependency(context.module, None, dependency_index)
            .expect("Harmony re-export must have a Module Graph connection");
        let internal_target = context
            .concatenation_scope
            .is_some_and(|scope| scope.contains(target));
        if !internal_target && !context.module_render_ids.contains_key(&target) {
            return;
        }
        let import_var = context
            .concatenation_scope
            .filter(|scope| scope.contains(target))
            .map_or_else(
                || {
                    import_var(
                        &dependency.module.request,
                        dependency.module.source_order.unwrap_or(0),
                    )
                },
                |scope| scope.exports_expression(target),
            );
        let exports_argument = context.exports_argument();
        if dependency.is_star {
            context.add_init_fragment(
                InitFragmentStage::StarReexport,
                format!(
                    "/* harmony reexport (unknown) */ for(const __WEBPACK_IMPORT_KEY__ in {import_var}) if(__WEBPACK_IMPORT_KEY__ !== \"default\" && __WEBPACK_IMPORT_KEY__ !== \"__esModule\") __webpack_require__.d({exports_argument}, {{ [__WEBPACK_IMPORT_KEY__]: () => ({import_var}[__WEBPACK_IMPORT_KEY__]) }});\n"
                ),
            );
        } else if let Some(name) = &dependency.name {
            let Some(used_name) = context.exports_info.get_used_name(name) else {
                return;
            };
            let expression = export_access_expression(&import_var, &dependency.ids);
            context.add_init_fragment(
                InitFragmentStage::Export,
                format!(
                    "__webpack_require__.d({exports_argument}, {{ {}: () => ({expression}) }});\n",
                    property_name(used_name),
                ),
            );
        }
    }
}
