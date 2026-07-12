// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/dependencies/ImportDependency.js

use serde::{Deserialize, Serialize};

use super::ModuleDependency;
use crate::{
    AsyncBlockOrigin, SourceRange,
    dependency_template::{DependencyTemplate, DependencyTemplateContext, json_render_id, replace},
    runtime::RuntimeRequirement,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImportDependency {
    pub module: ModuleDependency,
}

impl ImportDependency {
    pub fn new(
        request: impl Into<String>,
        range: SourceRange,
        source_order: Option<usize>,
    ) -> Self {
        let mut module = ModuleDependency::new(request, source_order);
        module.range = Some(range);
        Self { module }
    }

    pub fn range(&self) -> SourceRange {
        self.module
            .range
            .expect("import dependency should have range")
    }
}

pub(crate) struct ImportDependencyTemplate;

impl DependencyTemplate<ImportDependency> for ImportDependencyTemplate {
    fn source_ranges(&self, dependency: &ImportDependency) -> Vec<SourceRange> {
        dependency.module.range.into_iter().collect()
    }

    fn apply(
        &self,
        dependency: &ImportDependency,
        source: &mut rspack_sources::ReplaceSource,
        context: &mut DependencyTemplateContext<'_>,
    ) {
        context.add_runtime_requirement(RuntimeRequirement::Require);
        let block_index = context
            .origin_block
            .expect("Dynamic import must belong to an Async Block");
        let dependency_index = context
            .dependency_index
            .expect("Dynamic import must have a Dependency Index");
        let target = context
            .module_graph
            .module_for_dependency(context.module, Some(block_index), dependency_index)
            .expect("Dynamic import must have a Module Graph connection");
        let target_id = json_render_id(&context.module_render_ids[&target]);
        let origin = AsyncBlockOrigin {
            module: context.module,
            block: block_index,
        };
        let expression = if let Some(group_handle) = context.chunk_graph.block_chunk_group(origin) {
            context.add_runtime_requirement(RuntimeRequirement::EnsureChunk);
            let group = &context.chunk_graph.chunk_groups()[group_handle.index()];
            let chunk_handle = group
                .chunks()
                .first()
                .copied()
                .expect("Async Chunk Group must contain a Chunk");
            let chunk = context
                .chunk_graph
                .chunk(chunk_handle)
                .expect("Async Chunk must exist before Dynamic Import generation");
            let chunk_id = json_render_id(chunk.render_id());
            format!(
                "__webpack_require__.e({chunk_id}).then(__webpack_require__.bind(__webpack_require__, {target_id}))"
            )
        } else {
            format!(
                "Promise.resolve().then(__webpack_require__.bind(__webpack_require__, {target_id}))"
            )
        };
        replace(source, dependency.range(), expression);
    }
}
