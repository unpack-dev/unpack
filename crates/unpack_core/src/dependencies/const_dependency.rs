// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/ConstDependency.js

use serde::{Deserialize, Serialize};

use crate::{
    SourceRange,
    dependency_template::{DependencyTemplate, DependencyTemplateContext, replace},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConstDependency {
    pub expression: String,
    pub range: SourceRange,
}

impl ConstDependency {
    pub fn new(expression: impl Into<String>, range: SourceRange) -> Self {
        Self {
            expression: expression.into(),
            range,
        }
    }
}

pub(crate) struct ConstDependencyTemplate;

impl DependencyTemplate<ConstDependency> for ConstDependencyTemplate {
    fn source_ranges(&self, dependency: &ConstDependency) -> Vec<SourceRange> {
        vec![dependency.range]
    }

    fn apply(
        &self,
        dependency: &ConstDependency,
        source: &mut rspack_sources::ReplaceSource,
        _context: &mut DependencyTemplateContext<'_>,
    ) {
        replace(source, dependency.range, dependency.expression.clone());
    }
}
