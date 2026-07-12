use crate::{Compilation, compilation::CompilationHooks, compiler::CompilerHooks};

pub(crate) struct FlagDependencyExportsPlugin;

impl FlagDependencyExportsPlugin {
    pub fn apply(&self, hooks: &mut CompilerHooks) {
        hooks.compilation.tap(
            "FlagDependencyExportsPlugin",
            |compilation_hooks: &mut CompilationHooks| {
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
