// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/optimize/ConcatenatedModule.js

use rspack_sources::{ConcatSource, RawStringSource};
use rustc_hash::{FxHashMap, FxHashSet};

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
        let identifiers = self
            .modules
            .iter()
            .flat_map(|handle| {
                module_graph
                    .module(*handle)
                    .expect("a Concatenated Module must reference Modules in the Module Graph")
                    .identifiers()
            })
            .cloned()
            .collect::<FxHashSet<_>>();
        let scope = ConcatenationScope::new(&self.modules, &identifiers);
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
        source.add(RawStringSource::from(format!(
            "var {} = [",
            scope.exports_name()
        )));
        for (index, module) in self.modules.iter().enumerate() {
            if index > 0 {
                source.add(RawStringSource::from(", ".to_string()));
            }
            source.add(RawStringSource::from(
                if *module == self.root_module {
                    "__webpack_exports__"
                } else {
                    "{}"
                }
                .to_string(),
            ));
        }
        source.add(RawStringSource::from("];\n".to_string()));
        source.add(RawStringSource::from(format!(
            "var {initialized} = Array({length}).fill(false);\nvar {initializers} = Array({length});\n",
            initialized = scope.initialized_name(),
            initializers = scope.initializers_name(),
            length = self.modules.len(),
        )));
        for (handle, result) in &generated_modules {
            let index = scope.ordinal(*handle);
            source.add(RawStringSource::from(format!(
                "{initializers}[{index}] = () => {{\n  if ({initialized}[{index}]) return {exports}[{index}];\n  {initialized}[{index}] = true;\n  return ((__webpack_exports__, __webpack_require__) => {{\n",
                initializers = scope.initializers_name(),
                initialized = scope.initialized_name(),
                exports = scope.exports_name(),
            )));
            source.add(result.source().clone());
            source.add(RawStringSource::from(format!(
                "\n    return __webpack_exports__;\n  }})({exports}[{index}], __webpack_require__);\n}};\n",
                exports = scope.exports_name(),
            )));
        }
        source.add(RawStringSource::from(format!(
            "{}[{}]();\n",
            scope.initializers_name(),
            scope.ordinal(self.root_module),
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
        let scope = ConcatenationScope::new(concatenated.modules(), &FxHashSet::default());

        handles
            .into_iter()
            .map(|(name, handle)| {
                (
                    name,
                    (
                        scope.exports_expression(handle),
                        scope.init_expression(handle),
                    ),
                )
            })
            .collect()
    }
}
