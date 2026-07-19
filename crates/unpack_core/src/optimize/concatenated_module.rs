// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/optimize/ConcatenatedModule.js

use rspack_sources::{ConcatSource, RawStringSource};
use rustc_hash::FxHashMap;

use crate::{
    ChunkGraph, Error, ModuleGraph, ModuleHandle, code_generation_record::CodeGenerationResult,
    dependency_template::ConcatenationScope, id_assignment::RenderId,
    javascript::javascript_generator, normal_module_factory::ModuleGeneratorContext,
    runtime::RuntimeRequirements,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConcatenatedModule {
    root_module: ModuleHandle,
    modules: Vec<ModuleHandle>,
}

impl ConcatenatedModule {
    pub(crate) fn new(
        root_module: ModuleHandle,
        mut modules: Vec<ModuleHandle>,
        module_graph: &ModuleGraph,
    ) -> Self {
        modules.sort_by_key(|module| {
            module_graph
                .module(*module)
                .expect("a Concatenated Module must reference Modules in the Module Graph")
                .identity()
                .clone()
        });
        modules.dedup();
        assert!(
            modules.contains(&root_module),
            "a Concatenated Module must contain its root Module"
        );
        Self {
            root_module,
            modules,
        }
    }

    pub(crate) fn root_module(&self) -> ModuleHandle {
        self.root_module
    }

    pub(crate) fn modules(&self) -> &[ModuleHandle] {
        &self.modules
    }

    pub(crate) fn inner_modules(&self) -> impl Iterator<Item = ModuleHandle> + '_ {
        self.modules
            .iter()
            .copied()
            .filter(|module| *module != self.root_module)
    }

    pub(crate) fn code_generation(
        &self,
        module_graph: &ModuleGraph,
        chunk_graph: &ChunkGraph,
        module_render_ids: &FxHashMap<ModuleHandle, RenderId>,
    ) -> Result<CodeGenerationResult, Error> {
        let scope = ConcatenationScope::new(self.root_module, &self.modules);
        let mut generated_modules = Vec::with_capacity(self.modules.len());
        let mut runtime_requirements = RuntimeRequirements::default();
        for handle in &self.modules {
            let module = module_graph
                .module(*handle)
                .expect("a Concatenated Module must reference Modules in the Module Graph");
            let record = javascript_generator::generate_with_concatenation_scope(
                ModuleGeneratorContext {
                    module,
                    module_graph,
                    chunk_graph,
                    module_render_ids,
                },
                Some(&scope),
            )?;
            let result = record
                .into_result(module.source())
                .expect("concatenated Code Generation must match its Module source");
            runtime_requirements.extend(result.runtime_requirements());
            generated_modules.push((*handle, result));
        }

        // ADR 0148 records why the current source-preserving generator keeps
        // module scopes behind guarded initializers until resolved top-level
        // binding metadata is retained by the parser.
        let mut source = ConcatSource::default();
        source.add(RawStringSource::from(
            "// webpack-style concatenated module\n".to_string(),
        ));
        for inner in self.inner_modules() {
            source.add(RawStringSource::from(format!(
                "var {} = {{}};\n",
                scope.exports_name(inner)
            )));
        }
        for (handle, result) in &generated_modules {
            let exports_name = scope.exports_name(*handle);
            source.add(RawStringSource::from(format!(
                "var __webpack_initialized__{index} = false;\nvar {init} = () => {{\n  if (__webpack_initialized__{index}) return {exports_name};\n  __webpack_initialized__{index} = true;\n",
                index = scope.ordinal(*handle),
                init = scope.init_name(*handle),
            )));
            source.add(result.source().clone());
            source.add(RawStringSource::from(format!(
                "\n  return {exports_name};\n}};\n"
            )));
        }
        source.add(RawStringSource::from(format!(
            "{}();\n",
            scope.init_name(self.root_module)
        )));
        Ok(CodeGenerationResult::from_parts(
            source,
            runtime_requirements,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use crate::{
        ModuleIdentity, module::BuiltModuleContent, module_graph::BuildingModuleGraph,
        parser::ParsedModule,
    };

    use super::*;

    #[test]
    fn scope_names_are_stable_across_module_graph_insertion_orders() {
        let first = scope_names(&["root.js", "alpha.js", "omega.js"]);
        let second = scope_names(&["omega.js", "root.js", "alpha.js"]);

        assert_eq!(first, second);
    }

    fn scope_names(order: &[&str]) -> BTreeMap<String, (String, String)> {
        let mut building_module_graph = BuildingModuleGraph::default();
        let mut handles = BTreeMap::new();
        for name in order {
            let handle = building_module_graph.add_module(ModuleIdentity::new(name), None);
            building_module_graph
                .finish_module_build(
                    handle,
                    Arc::new(BuiltModuleContent::new(
                        ParsedModule::default(),
                        String::new(),
                    )),
                )
                .expect("synthetic module should exist");
            handles.insert((*name).to_string(), handle);
        }
        let module_graph = building_module_graph.finish();
        let root = handles["root.js"];
        let concatenated =
            ConcatenatedModule::new(root, handles.values().copied().collect(), &module_graph);
        let scope = ConcatenationScope::new(root, concatenated.modules());

        handles
            .into_iter()
            .map(|(name, handle)| (name, (scope.exports_name(handle), scope.init_name(handle))))
            .collect()
    }
}
