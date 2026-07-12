use std::{future::Future, pin::Pin, sync::Arc};

use super::Compilation;

type AsyncTap = Arc<
    dyn for<'a> Fn(&'a mut Compilation) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
        + Send
        + Sync,
>;
type SyncTap = Arc<dyn Fn(&mut Compilation) + Send + Sync>;

#[derive(Default, Clone)]
pub(crate) struct AsyncCompilationHook {
    taps: Vec<(&'static str, AsyncTap)>,
}

impl AsyncCompilationHook {
    pub fn tap(
        &mut self,
        name: &'static str,
        tap: impl for<'a> Fn(&'a mut Compilation) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
        + Send
        + Sync
        + 'static,
    ) {
        self.taps.push((name, Arc::new(tap)));
    }

    pub async fn call(&self, compilation: &mut Compilation) {
        for (_, tap) in &self.taps {
            tap(compilation).await;
        }
    }
}

#[derive(Default, Clone)]
pub(crate) struct SyncCompilationHook {
    taps: Vec<(&'static str, SyncTap)>,
}

impl SyncCompilationHook {
    pub fn tap(
        &mut self,
        name: &'static str,
        tap: impl Fn(&mut Compilation) + Send + Sync + 'static,
    ) {
        self.taps.push((name, Arc::new(tap)));
    }

    pub fn call(&self, compilation: &mut Compilation) {
        for (_, tap) in &self.taps {
            tap(compilation);
        }
    }
}

#[derive(Default, Clone)]
pub(crate) struct CompilationHooks {
    pub finish_modules: AsyncCompilationHook,
    pub optimize_dependencies: SyncCompilationHook,
}

impl std::fmt::Debug for CompilationHooks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CompilationHooks")
            .field("finish_modules_taps", &self.finish_modules.taps.len())
            .field(
                "optimize_dependencies_taps",
                &self.optimize_dependencies.taps.len(),
            )
            .finish()
    }
}
