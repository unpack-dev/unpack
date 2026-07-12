use std::sync::Arc;

use crate::compilation::CompilationHookSet;

type RegistrationTap = Arc<dyn Fn(&mut CompilationHookSet) + Send + Sync>;

#[derive(Default, Clone)]
pub(crate) struct CompilerHookSet {
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
        tap: impl Fn(&mut CompilationHookSet) + Send + Sync + 'static,
    ) {
        self.taps.push((name, Arc::new(tap)));
    }

    pub fn call(&self, hooks: &mut CompilationHookSet) {
        for (_, tap) in &self.taps {
            tap(hooks);
        }
    }
}

impl std::fmt::Debug for CompilerHookSet {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CompilerHookSet")
            .field("compilation_taps", &self.compilation.taps.len())
            .finish()
    }
}
