// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/EntryDependency.js

use serde::{Deserialize, Serialize};

use super::ModuleDependency;
use crate::dependency_template::{DependencyTemplate, DependencyTemplateContext};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntryDependency {
    pub module: ModuleDependency,
}

impl EntryDependency {
    pub fn new(request: impl Into<String>) -> Self {
        Self {
            module: ModuleDependency::new(request, None),
        }
    }
}

pub(crate) struct EntryDependencyTemplate;

impl DependencyTemplate<EntryDependency> for EntryDependencyTemplate {
    fn apply(
        &self,
        _dependency: &EntryDependency,
        _source: &mut rspack_sources::ReplaceSource,
        _context: &mut DependencyTemplateContext<'_>,
    ) {
    }
}
