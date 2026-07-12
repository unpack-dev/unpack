use std::sync::Arc;

use crate::compilation::CompilationHooks;

type RegistrationTap = Arc<dyn Fn(&mut CompilationHooks) + Send + Sync>;

#[derive(Default, Clone)]
pub(crate) struct CompilerHooks {
    pub compilation: CompilationRegistrationHook,
}

#[derive(Default, Clone)]
pub(crate) struct CompilationRegistrationHook {
    taps: Vec<(&'static str, RegistrationTap)>,
}

impl CompilationRegistrationHook {
    pub fn tap(
        &mut self,
        name: &'static str,
        tap: impl Fn(&mut CompilationHooks) + Send + Sync + 'static,
    ) {
        self.taps.push((name, Arc::new(tap)));
    }

    pub fn call(&self, hooks: &mut CompilationHooks) {
        for (_, tap) in &self.taps {
            tap(hooks);
        }
    }
}

impl std::fmt::Debug for CompilerHooks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CompilerHooks")
            .field("compilation_taps", &self.compilation.taps.len())
            .finish()
    }
}
