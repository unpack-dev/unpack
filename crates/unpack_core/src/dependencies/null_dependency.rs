// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/NullDependency.js

use serde::{Deserialize, Serialize};

use crate::dependency_template::{DependencyTemplate, DependencyTemplateContext};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NullDependency;

pub(crate) struct NullDependencyTemplate;

impl DependencyTemplate<NullDependency> for NullDependencyTemplate {
    fn apply(
        &self,
        _dependency: &NullDependency,
        _source: &mut rspack_sources::ReplaceSource,
        _context: &mut DependencyTemplateContext<'_>,
    ) {
    }
}
