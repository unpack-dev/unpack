// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/javascript/JavascriptGenerator.js

use rspack_sources::{OriginalSource, ReplaceSource};

use crate::{
    AsyncDependenciesBlockIndex, Dependency, DependencyIndex, Result,
    code_generation_record::{
        CodeGenerationRecord, CodeGenerationReplacement, CodeGenerationSource,
    },
    dependency_template::{ConcatenationScope, DependencyTemplateContext},
    init_fragment::{InitFragment, InitFragmentStage},
    normal_module_factory::ModuleGeneratorContext,
    runtime::{RuntimeRequirement, RuntimeRequirements},
};

pub(crate) fn generate(context: ModuleGeneratorContext<'_>) -> Result<CodeGenerationRecord> {
    generate_with_concatenation_scope(context, None)
}

pub(crate) fn generate_with_concatenation_scope(
    context: ModuleGeneratorContext<'_>,
    concatenation_scope: Option<&ConcatenationScope<'_>>,
) -> Result<CodeGenerationRecord> {
    let ModuleGeneratorContext {
        module,
        module_graph,
        chunk_graph,
        module_render_ids,
    } = context;
    let module_handle = module.handle();
    let module_render_name = module_render_ids
        .get(&module_handle)
        .map(ToString::to_string)
        .unwrap_or_else(|| module.identity().resource.to_string_lossy().into_owned());
    let mut source = ReplaceSource::new(OriginalSource::new(
        module.source(),
        module_render_name.as_str(),
    ));
    let mut init_fragments = Vec::new();
    let mut runtime_requirements = RuntimeRequirements::default();
    if module.is_harmony() {
        runtime_requirements.insert(RuntimeRequirement::MakeNamespaceObject);
        let exports_argument = concatenation_scope.map_or_else(
            || "__webpack_exports__".to_string(),
            |scope| scope.exports_name(module_handle),
        );
        init_fragments.push(InitFragment::new(
            InitFragmentStage::Compatibility,
            init_fragments.len(),
            format!("__webpack_require__.r({exports_argument});\n"),
        ));
    }

    {
        let mut apply_template =
            |dependency: &Dependency,
             origin_block: Option<AsyncDependenciesBlockIndex>,
             dependency_index: Option<DependencyIndex>| {
                let mut context = DependencyTemplateContext {
                    module: module_handle,
                    origin_block,
                    dependency_index,
                    module_graph,
                    chunk_graph,
                    exports_info: module_graph.exports_info(module_handle),
                    module_render_ids,
                    concatenation_scope,
                    runtime_requirements: &mut runtime_requirements,
                    init_fragments: &mut init_fragments,
                };
                dependency.apply_template(&mut source, &mut context)
            };

        for dependency in module.presentational_dependencies() {
            apply_template(dependency, None, None)?;
        }
        for (dependency_index, dependency) in module.dependencies().iter().enumerate() {
            apply_template(
                dependency,
                None,
                Some(DependencyIndex::new(dependency_index)),
            )?;
        }
        for (block_index, block) in module.blocks().iter().enumerate() {
            for (dependency_index, dependency) in block.dependencies().iter().enumerate() {
                apply_template(
                    dependency,
                    Some(AsyncDependenciesBlockIndex::new(block_index)),
                    Some(DependencyIndex::new(dependency_index)),
                )?;
            }
        }
    }

    let init = InitFragment::render(init_fragments);
    Ok(
        CodeGenerationRecord::new(CodeGenerationSource::OriginalWithReplacements {
            prefix: init,
            original_source_len: u32::try_from(module.source_len())
                .expect("Module source length must fit the Code Generation cache format"),
            original_name: module_render_name,
            replacements: source
                .replacements()
                .iter()
                .map(CodeGenerationReplacement::from)
                .collect(),
            suffix: String::new(),
        })
        .with_runtime_requirements(runtime_requirements),
    )
}
