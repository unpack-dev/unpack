// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/HarmonyImportSpecifierDependency.js

use serde::{Deserialize, Serialize};

use super::ModuleDependency;
use crate::{
    SourceRange,
    dependency_template::{
        DependencyTemplate, DependencyTemplateContext, import_expression, replace,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HarmonyImportSpecifierDependency {
    pub module: ModuleDependency,
    pub ids: Vec<String>,
    pub name: String,
    pub usage_range: SourceRange,
    pub shorthand: bool,
}

impl HarmonyImportSpecifierDependency {
    pub fn new(
        request: impl Into<String>,
        source_order: usize,
        ids: Vec<String>,
        name: impl Into<String>,
        usage_range: SourceRange,
    ) -> Self {
        Self {
            module: ModuleDependency::new(request, Some(source_order)),
            ids,
            name: name.into(),
            usage_range,
            shorthand: false,
        }
    }
}

pub(crate) struct HarmonyImportSpecifierDependencyTemplate;

impl DependencyTemplate<HarmonyImportSpecifierDependency>
    for HarmonyImportSpecifierDependencyTemplate
{
    fn source_ranges(&self, dependency: &HarmonyImportSpecifierDependency) -> Vec<SourceRange> {
        let mut ranges = dependency.module.range.into_iter().collect::<Vec<_>>();
        ranges.push(dependency.usage_range);
        ranges
    }

    fn apply(
        &self,
        dependency: &HarmonyImportSpecifierDependency,
        source: &mut rspack_sources::ReplaceSource,
        context: &mut DependencyTemplateContext<'_>,
    ) {
        let dependency_index = context
            .dependency_index
            .expect("Harmony import specifier must have a Dependency Index");
        context
            .module_graph
            .module_for_dependency(context.module, None, dependency_index)
            .expect("Harmony import specifier must have a Module Graph connection");
        let expression = import_expression(
            &dependency.module.request,
            dependency.module.source_order.unwrap_or(0),
            &dependency.ids,
        );
        let expression = if dependency.shorthand {
            format!("{}: {expression}", dependency.name)
        } else {
            expression
        };
        replace(source, dependency.usage_range, expression);
    }
}
