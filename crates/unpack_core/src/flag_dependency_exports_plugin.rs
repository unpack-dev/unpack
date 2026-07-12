use crate::{Compilation, compilation::CompilationHookSet, compiler::CompilerHookSet};

pub(crate) struct FlagDependencyExportsPlugin;

impl FlagDependencyExportsPlugin {
    pub fn apply(&self, hooks: &mut CompilerHookSet) {
        hooks.compilation.tap(
            "FlagDependencyExportsPlugin",
            |compilation_hooks: &mut CompilationHookSet| {
                compilation_hooks
                    .finish_modules
                    .tap("FlagDependencyExportsPlugin", |compilation| {
                        Box::pin(async move { flag_dependency_exports(compilation) })
                    });
            },
        );
    }
}

fn flag_dependency_exports(compilation: &mut Compilation) {
    let module_computation_cache = compilation.module_computation_cache().cloned();
    let handles = compilation
        .module_graph()
        .modules()
        .iter()
        .map(|module| module.handle())
        .collect::<Vec<_>>();
    for handle in handles {
        if let Some(module) = compilation.module_graph_mut().module_mut(handle) {
            if let Some(exports_info) = module_computation_cache
                .as_ref()
                .and_then(|cache| cache.get_provided_exports(module.identity()))
            {
                *module.exports_info_mut() = exports_info;
                continue;
            }
            module.analyze_provided_exports();
            if let Some(cache) = &module_computation_cache {
                cache.store_provided_exports(module.identity(), module.exports_info().clone());
            }
        }
    }
}
