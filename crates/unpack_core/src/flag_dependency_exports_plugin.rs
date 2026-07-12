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
    let handles = compilation
        .module_graph()
        .modules()
        .iter()
        .map(|module| module.handle())
        .collect::<Vec<_>>();
    for handle in handles {
        if let Some(module) = compilation.module_graph_mut().module_mut(handle) {
            module.analyze_provided_exports();
        }
    }
}
