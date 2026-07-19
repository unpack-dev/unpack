// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/DependencyTemplate.js

use rustc_hash::FxHashMap;

use rspack_sources::ReplaceSource;

use crate::{
    AsyncDependenciesBlockIndex, ChunkGraph, DependencyIndex, Error, ExportsInfo, ModuleGraph,
    ModuleHandle, SourceRange,
    id_assignment::RenderId,
    init_fragment::{InitFragment, InitFragmentStage},
    runtime::{RuntimeRequirement, RuntimeRequirements},
};

pub(crate) trait DependencyTemplate<D> {
    fn source_ranges(&self, _dependency: &D) -> Vec<SourceRange> {
        Vec::new()
    }

    fn apply(
        &self,
        dependency: &D,
        source: &mut ReplaceSource,
        context: &mut DependencyTemplateContext<'_>,
    );
}

pub(crate) struct DependencyTemplateContext<'a> {
    pub(crate) module: ModuleHandle,
    pub(crate) origin_block: Option<AsyncDependenciesBlockIndex>,
    pub(crate) dependency_index: Option<DependencyIndex>,
    pub(crate) module_graph: &'a ModuleGraph,
    pub(crate) chunk_graph: &'a ChunkGraph,
    pub(crate) exports_info: &'a ExportsInfo,
    pub(crate) module_render_ids: &'a FxHashMap<ModuleHandle, RenderId>,
    pub(crate) concatenation_scope: Option<&'a ConcatenationScope<'a>>,
    pub(crate) runtime_requirements: &'a mut RuntimeRequirements,
    pub(crate) init_fragments: &'a mut Vec<InitFragment>,
}

impl DependencyTemplateContext<'_> {
    pub(crate) fn add_runtime_requirement(&mut self, requirement: RuntimeRequirement) {
        self.runtime_requirements.insert(requirement);
    }

    pub(crate) fn add_init_fragment(&mut self, stage: InitFragmentStage, content: String) {
        self.init_fragments
            .push(InitFragment::new(stage, self.init_fragments.len(), content));
    }

    pub(crate) fn exports_argument(&self) -> String {
        self.concatenation_scope.map_or_else(
            || "__webpack_exports__".to_string(),
            |scope| scope.exports_name(self.module),
        )
    }

    fn validate_source_ranges(
        &self,
        ranges: impl IntoIterator<Item = SourceRange>,
    ) -> Result<(), Error> {
        let module = self
            .module_graph
            .module(self.module)
            .expect("Dependency Template origin Module must exist in the Module Graph");
        for range in ranges {
            if range.start > range.end || range.end as usize > module.source_len() {
                return Err(Error::CodeGeneration {
                    module: self.module,
                    path: module.identity().resource.clone(),
                    message: format!(
                        "dependency source range {}..{} exceeds module source length {}",
                        range.start,
                        range.end,
                        module.source_len()
                    ),
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ConcatenationScope<'a> {
    root: ModuleHandle,
    modules: &'a [ModuleHandle],
}

impl<'a> ConcatenationScope<'a> {
    pub(crate) fn new(root: ModuleHandle, modules: &'a [ModuleHandle]) -> Self {
        Self { root, modules }
    }

    pub(crate) fn contains(&self, module: ModuleHandle) -> bool {
        self.modules.contains(&module)
    }

    pub(crate) fn ordinal(&self, module: ModuleHandle) -> usize {
        self.modules
            .iter()
            .position(|candidate| *candidate == module)
            .expect("a Concatenation Scope must contain the requested Module")
    }

    pub(crate) fn exports_name(&self, module: ModuleHandle) -> String {
        if module == self.root {
            "__webpack_exports__".to_string()
        } else {
            format!("__webpack_exports__{}", self.ordinal(module))
        }
    }

    pub(crate) fn init_name(&self, module: ModuleHandle) -> String {
        format!("__webpack_init__{}", self.ordinal(module))
    }
}

pub(crate) fn apply_dependency_template<D>(
    template: &impl DependencyTemplate<D>,
    dependency: &D,
    source: &mut ReplaceSource,
    context: &mut DependencyTemplateContext<'_>,
) -> Result<(), Error> {
    context.validate_source_ranges(template.source_ranges(dependency))?;
    template.apply(dependency, source, context);
    Ok(())
}

pub(crate) fn replace(source: &mut ReplaceSource, range: SourceRange, content: String) {
    source.replace(range.start, range.end, content, None);
}

pub(crate) fn import_var(request: &str, source_order: usize) -> String {
    let ident = sanitize_identifier(request);
    let index = source_order.saturating_sub(1);
    format!("_{ident}__WEBPACK_IMPORTED_MODULE_{index}__")
}

pub(crate) fn import_expression(request: &str, source_order: usize, ids: &[String]) -> String {
    let import_var = import_var(request, source_order);
    export_access_expression(&import_var, ids)
}

pub(crate) fn export_access_expression(base: &str, ids: &[String]) -> String {
    let mut expression = base.to_string();
    for id in ids {
        expression.push_str(&property_access(id));
    }
    expression
}

fn sanitize_identifier(value: &str) -> String {
    let mut ident = value
        .trim_start_matches("./")
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    if ident.is_empty() {
        ident.push_str("module");
    }
    if ident.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        ident.insert(0, '_');
    }
    ident
}

fn property_access(property: &str) -> String {
    if is_identifier(property) {
        format!(".{property}")
    } else {
        format!("[{}]", json_string(property))
    }
}

pub(crate) fn property_name(property: &str) -> String {
    if is_identifier(property) {
        property.to_string()
    } else {
        format!("[{}]", json_string(property))
    }
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first == '$' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

pub(crate) fn json_string(value: &str) -> String {
    simd_json::to_string(value).expect("JavaScript string input must serialize as JSON")
}

pub(crate) fn json_render_id(render_id: &RenderId) -> String {
    match render_id {
        RenderId::String(value) => json_string(value),
        RenderId::Number(value) => value.to_string(),
    }
}
