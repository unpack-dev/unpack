// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/FlagDependencyExportsPlugin.js

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
        let module = compilation
            .module_graph()
            .module(handle)
            .expect("a Module Graph handle should address a Module");
        let identity = module.identity().clone();
        let cached_exports_info = module_computation_cache
            .as_ref()
            .and_then(|cache| cache.get_provided_exports(&identity));
        let exports_info = cached_exports_info.unwrap_or_else(|| {
            let exports_info = module.provided_exports();
            if let Some(cache) = &module_computation_cache {
                cache.store_provided_exports(&identity, exports_info.clone());
            }
            exports_info
        });
        *compilation.module_graph_mut().exports_info_mut(handle) = exports_info;
    }
}
