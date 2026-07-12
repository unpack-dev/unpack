// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/HarmonyExportHeaderDependency.js

use serde::{Deserialize, Serialize};

use crate::{
    SourceRange,
    dependency_template::{DependencyTemplate, DependencyTemplateContext, replace},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HarmonyExportHeaderDependency {
    pub declaration_range: Option<SourceRange>,
    pub statement_range: SourceRange,
}

impl HarmonyExportHeaderDependency {
    pub fn new(declaration_range: Option<SourceRange>, statement_range: SourceRange) -> Self {
        Self {
            declaration_range,
            statement_range,
        }
    }
}

pub(crate) struct HarmonyExportHeaderDependencyTemplate;

impl DependencyTemplate<HarmonyExportHeaderDependency> for HarmonyExportHeaderDependencyTemplate {
    fn source_ranges(&self, dependency: &HarmonyExportHeaderDependency) -> Vec<SourceRange> {
        let mut ranges = vec![dependency.statement_range];
        ranges.extend(dependency.declaration_range);
        ranges
    }

    fn apply(
        &self,
        dependency: &HarmonyExportHeaderDependency,
        source: &mut rspack_sources::ReplaceSource,
        _context: &mut DependencyTemplateContext<'_>,
    ) {
        let end = dependency
            .declaration_range
            .map(|range| range.start)
            .unwrap_or(dependency.statement_range.end);
        replace(
            source,
            SourceRange::new(dependency.statement_range.start, end),
            String::new(),
        );
    }
}
