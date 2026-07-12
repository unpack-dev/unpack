// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/javascript/JavascriptGenerator.js

use rspack_sources::{OriginalSource, ReplaceSource};

use crate::{
    AsyncDependenciesBlockIndex, DependencyIndex, Result,
    code_generation::{
        apply_dependency_template, apply_harmony_compatibility_template, render_init_fragments,
    },
    code_generation_record::{
        CodeGenerationRecord, CodeGenerationReplacement, CodeGenerationSource,
    },
    normal_module_factory::ModuleGeneratorContext,
    runtime::RuntimeRequirements,
};

pub(crate) fn generate(context: ModuleGeneratorContext<'_>) -> Result<CodeGenerationRecord> {
    let ModuleGeneratorContext {
        module,
        module_graph,
        chunk_graph,
        module_render_ids,
    } = context;
    let module_handle = module.handle();
    let module_render_id = &module_render_ids[&module_handle];
    let module_render_name = module_render_id.to_string();
    let mut source = ReplaceSource::new(OriginalSource::new(
        module.source(),
        module_render_name.as_str(),
    ));
    let mut init_fragments = Vec::new();
    let mut runtime_requirements = RuntimeRequirements::default();
    if module.is_harmony() {
        apply_harmony_compatibility_template(&mut runtime_requirements, &mut init_fragments);
    }

    for dependency in module.presentational_dependencies() {
        apply_dependency_template(
            dependency,
            module_handle,
            None,
            None,
            module_graph,
            chunk_graph,
            module.exports_info(),
            module_render_ids,
            &mut runtime_requirements,
            &mut source,
            &mut init_fragments,
        )?;
    }
    for (dependency_index, dependency) in module.dependencies().iter().enumerate() {
        apply_dependency_template(
            dependency,
            module_handle,
            None,
            Some(DependencyIndex::new(dependency_index)),
            module_graph,
            chunk_graph,
            module.exports_info(),
            module_render_ids,
            &mut runtime_requirements,
            &mut source,
            &mut init_fragments,
        )?;
    }
    for (block_index, block) in module.blocks().iter().enumerate() {
        for (dependency_index, dependency) in block.dependencies().iter().enumerate() {
            apply_dependency_template(
                dependency,
                module_handle,
                Some(AsyncDependenciesBlockIndex::new(block_index)),
                Some(DependencyIndex::new(dependency_index)),
                module_graph,
                chunk_graph,
                module.exports_info(),
                module_render_ids,
                &mut runtime_requirements,
                &mut source,
                &mut init_fragments,
            )?;
        }
    }

    let init = render_init_fragments(init_fragments);
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
