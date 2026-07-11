use std::{
    path::PathBuf,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use crate::{
    CacheOptions, Compilation, InfrastructureLoggingOptions, LoaderRunner, ModuleRule,
    ResolveOptions, Result, SnapshotOptions, UnpackResolver, build_cache::BuildCache,
};
use tracing::Instrument;

pub const DEFAULT_EXTENSIONS: &[&str] = &[".ts", ".tsx", ".js", ".jsx"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub request: String,
}

impl Entry {
    pub fn new(name: impl Into<String>, request: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            request: request.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompilerOptions {
    pub context: PathBuf,
    pub entries: Vec<Entry>,
    pub cache: CacheOptions,
    pub resolve: ResolveOptions,
    pub snapshot: SnapshotOptions,
    pub infrastructure_logging: InfrastructureLoggingOptions,
    pub module_rules: Vec<ModuleRule>,
    pub loader_runner: Option<Arc<dyn LoaderRunner>>,
    pub parallelism: usize,
    pub sourcemap: bool,
}

impl CompilerOptions {
    pub fn new(context: impl Into<PathBuf>, entries: Vec<Entry>) -> Self {
        Self {
            context: normalize_context(context.into()),
            entries,
            cache: CacheOptions::default(),
            resolve: default_resolve_options(),
            snapshot: SnapshotOptions::default(),
            infrastructure_logging: InfrastructureLoggingOptions::disabled(),
            module_rules: Vec::new(),
            loader_runner: None,
      parallelism: std::thread::available_parallelism()
        .map_or(16, |parallelism| (parallelism.get() * 4).clamp(16, 32)),
            sourcemap: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Compiler {
    options: CompilerOptions,
    build_cache: BuildCache,
    cache_lifecycle: Arc<CacheLifecycle>,
}

#[derive(Debug)]
pub struct PendingCompilation {
    compilation: Option<Compilation>,
    cache_activity: Option<CacheRunActivity>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CacheLifecycleOutcome {
    diagnostic: Option<String>,
    infrastructure_log_events: Vec<crate::InfrastructureLogEvent>,
}

impl CacheLifecycleOutcome {
    pub fn diagnostic(&self) -> Option<&str> {
        self.diagnostic.as_deref()
    }

    pub fn infrastructure_log_events(&self) -> &[crate::InfrastructureLogEvent] {
        &self.infrastructure_log_events
    }
}

impl PendingCompilation {
    pub fn compilation(&self) -> &Compilation {
        self.compilation
            .as_ref()
            .expect("pending compilation should exist until finish")
    }

    pub fn finish(mut self) -> Compilation {
        let compilation = self
            .compilation
            .take()
            .expect("pending compilation should only be finished once");
        drop(self.cache_activity.take());
        compilation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheIdleReason {
    Ordinary,
    LargeChange,
}

#[derive(Debug, Clone, Copy)]
struct CacheIdleTimeouts {
    ordinary: Duration,
    initial_store: Duration,
    after_large_change: Duration,
}

impl CacheIdleTimeouts {
    fn from_options(options: &CacheOptions) -> Self {
        Self {
            ordinary: Duration::from_millis(u64::from(options.idle_timeout.unwrap_or(60_000))),
            initial_store: Duration::from_millis(u64::from(
                options.idle_timeout_for_initial_store.unwrap_or(5_000),
            )),
            after_large_change: Duration::from_millis(u64::from(
                options.idle_timeout_after_large_changes.unwrap_or(1_000),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheActivity {
    Ready,
    Active { run_id: u64 },
    Idle,
    ShuttingDown,
    Closed,
}

#[derive(Debug, Clone, Copy)]
struct ScheduledCacheFlush {
    token: u64,
    deadline: tokio::time::Instant,
    reason: CacheFlushReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheFlushReason {
    InitialStore,
    Ordinary,
    LargeChange,
}

#[derive(Debug, Clone, Copy)]
struct InFlightCacheFlush {
    target_generation: u64,
}

#[derive(Debug)]
struct CacheLifecycleState {
    activity: CacheActivity,
    shutdown_run_id: Option<u64>,
    scheduled: Option<ScheduledCacheFlush>,
    flush: Option<InFlightCacheFlush>,
    diagnostic: Option<String>,
    shutdown_outcome: Option<CacheLifecycleOutcome>,
    initial_store_deadline: Option<tokio::time::Instant>,
    ordinary_deadline: Option<tokio::time::Instant>,
    large_change_deadline: Option<tokio::time::Instant>,
    next_run_id: u64,
    next_timer_token: u64,
}

#[derive(Debug)]
struct CacheLifecycle {
    build_cache: BuildCache,
    timeouts: CacheIdleTimeouts,
    state: Mutex<CacheLifecycleState>,
    changed: tokio::sync::watch::Sender<u64>,
}

impl CacheLifecycle {
    fn new(build_cache: BuildCache, options: &CacheOptions) -> Arc<Self> {
        let (changed, _) = tokio::sync::watch::channel(0);
        Arc::new(Self {
            build_cache,
            timeouts: CacheIdleTimeouts::from_options(options),
            state: Mutex::new(CacheLifecycleState {
                activity: CacheActivity::Ready,
                shutdown_run_id: None,
                scheduled: None,
                flush: None,
                diagnostic: None,
                shutdown_outcome: None,
                initial_store_deadline: None,
                ordinary_deadline: None,
                large_change_deadline: None,
                next_run_id: 1,
                next_timer_token: 1,
            }),
            changed,
        })
    }

    fn end_idle(self: &Arc<Self>, idle_reason: CacheIdleReason) -> Result<CacheRunActivity> {
        let mut state = self
            .state
            .lock()
            .expect("cache lifecycle mutex should not be poisoned");
        match state.activity {
            CacheActivity::Ready | CacheActivity::Idle => {}
            CacheActivity::Active { .. } => return Err(crate::Error::CompilerBusy),
            CacheActivity::ShuttingDown | CacheActivity::Closed => {
                return Err(crate::Error::CompilerClosed);
            }
        }
        state.scheduled = None;
        state.ordinary_deadline = None;
        state.large_change_deadline = None;
        let run_id = state.next_run_id;
        state.next_run_id = state.next_run_id.saturating_add(1);
        state.activity = CacheActivity::Active { run_id };
        drop(state);
        self.notify_changed();
        Ok(CacheRunActivity {
            lifecycle: Arc::downgrade(self),
            run_id,
            idle_reason,
        })
    }

    fn begin_idle(self: &Arc<Self>, run_id: u64, idle_reason: CacheIdleReason) {
        let should_schedule = {
            let mut state = self
                .state
                .lock()
                .expect("cache lifecycle mutex should not be poisoned");
            if state.activity == CacheActivity::ShuttingDown
                && state.shutdown_run_id == Some(run_id)
            {
                state.shutdown_run_id = None;
                false
            } else if state.activity != (CacheActivity::Active { run_id }) {
                return;
            } else {
                state.activity = CacheActivity::Idle;
                if self.build_cache.pending_generation().is_some() {
                    let now = tokio::time::Instant::now();
                    state.ordinary_deadline = Some(now + self.timeouts.ordinary);
                    if self.build_cache.initial_store_pending()
                        && state.initial_store_deadline.is_none()
                    {
                        state.initial_store_deadline = Some(now + self.timeouts.initial_store);
                    }
                    if idle_reason == CacheIdleReason::LargeChange {
                        state.large_change_deadline = Some(now + self.timeouts.after_large_change);
                    }
                    state.flush.is_none()
                } else {
                    state.initial_store_deadline = None;
                    state.ordinary_deadline = None;
                    state.large_change_deadline = None;
                    false
                }
            }
        };
        self.notify_changed();
        if should_schedule {
            self.schedule_idle_timer();
        }
    }

    fn schedule_idle_timer(self: &Arc<Self>) {
        let plan = {
            let mut state = self
                .state
                .lock()
                .expect("cache lifecycle mutex should not be poisoned");
            if state.activity != CacheActivity::Idle
                || state.flush.is_some()
                || state.scheduled.is_some()
            {
                return;
            }
            let Some(target_generation) = self.build_cache.pending_generation() else {
                state.initial_store_deadline = None;
                state.ordinary_deadline = None;
                state.large_change_deadline = None;
                return;
            };
            let now = tokio::time::Instant::now();
            if state.ordinary_deadline.is_none() {
                state.ordinary_deadline = Some(now + self.timeouts.ordinary);
            }
            if self.build_cache.initial_store_pending() && state.initial_store_deadline.is_none() {
                state.initial_store_deadline = Some(now + self.timeouts.initial_store);
            }
            let (reason, deadline) = [
                (CacheFlushReason::InitialStore, state.initial_store_deadline),
                (CacheFlushReason::LargeChange, state.large_change_deadline),
                (CacheFlushReason::Ordinary, state.ordinary_deadline),
            ]
            .into_iter()
            .filter_map(|(reason, deadline)| deadline.map(|deadline| (reason, deadline)))
            .min_by_key(|(_, deadline)| *deadline)
            .expect("dirty cache should always have an idle deadline");
            let token = state.next_timer_token;
            state.next_timer_token = state.next_timer_token.saturating_add(1);
            state.scheduled = Some(ScheduledCacheFlush {
                token,
                deadline,
                reason,
            });
            (token, deadline, target_generation)
        };
        self.notify_changed();
        let (token, deadline, target_generation) = plan;
        let lifecycle = Arc::downgrade(self);
        tokio::spawn(async move {
            tokio::time::sleep_until(deadline).await;
            let Some(lifecycle) = lifecycle.upgrade() else {
                return;
            };
            lifecycle.fire_timer(token, target_generation).await;
        });
    }

    async fn fire_timer(self: Arc<Self>, token: u64, target_generation: u64) {
        {
            let mut state = self
                .state
                .lock()
                .expect("cache lifecycle mutex should not be poisoned");
            if state.activity != CacheActivity::Idle {
                return;
            }
            let Some(scheduled) = state.scheduled.filter(|scheduled| scheduled.token == token)
            else {
                return;
            };
            if tokio::time::Instant::now() < scheduled.deadline {
                return;
            }
            state.scheduled = None;
            match scheduled.reason {
                CacheFlushReason::InitialStore => state.initial_store_deadline = None,
                CacheFlushReason::Ordinary => state.ordinary_deadline = None,
                CacheFlushReason::LargeChange => state.large_change_deadline = None,
            }
        }
        self.notify_changed();
        let Some(current_target) = self.build_cache.pending_generation() else {
            return;
        };
        {
            let mut state = self
                .state
                .lock()
                .expect("cache lifecycle mutex should not be poisoned");
            if state.activity != CacheActivity::Idle || state.flush.is_some() {
                return;
            }
            state.flush = Some(InFlightCacheFlush {
                target_generation: current_target.max(target_generation),
            });
        }
        self.notify_changed();
        self.perform_flush(current_target.max(target_generation))
            .await;
    }

    async fn perform_flush(self: &Arc<Self>, target_generation: u64) {
        let build_cache = self.build_cache.clone();
        let result = match tokio::task::spawn_blocking(move || {
            build_cache.publish_generation(target_generation)
        })
        .await
        {
            Ok(result) => result.map_err(|error| error.to_string()),
            Err(error) => Err(format!("cache publication task failed: {error}")),
        };
        let mut state = self
            .state
            .lock()
            .expect("cache lifecycle mutex should not be poisoned");
        if state
            .flush
            .is_some_and(|flush| flush.target_generation == target_generation)
        {
            state.flush = None;
        }
        let should_schedule = match result {
            Ok(()) => {
                if self.build_cache.pending_generation().is_none() {
                    state.initial_store_deadline = None;
                    state.ordinary_deadline = None;
                    state.large_change_deadline = None;
                    false
                } else {
                    state.activity == CacheActivity::Idle && state.scheduled.is_none()
                }
            }
            Err(error) => {
                state.diagnostic = Some(error);
                false
            }
        };
        drop(state);
        self.notify_changed();
        if should_schedule {
            self.schedule_idle_timer();
        }
    }

    async fn settle(self: &Arc<Self>) -> CacheLifecycleOutcome {
        let mut changed = self.changed.subscribe();
        loop {
            enum Action {
                Start(u64),
                Wait,
                Done(CacheLifecycleOutcome),
            }

            let action = {
                let mut state = self
                    .state
                    .lock()
                    .expect("cache lifecycle mutex should not be poisoned");
                state.scheduled = None;
                state.ordinary_deadline = None;
                state.large_change_deadline = None;
                if let Some(diagnostic) = state.diagnostic.take() {
                    if matches!(state.activity, CacheActivity::Idle) {
                        state.activity = CacheActivity::Ready;
                    }
                    Action::Done(CacheLifecycleOutcome {
                        diagnostic: Some(diagnostic),
                        ..CacheLifecycleOutcome::default()
                    })
                } else if matches!(state.activity, CacheActivity::Active { .. })
                    || state.flush.is_some()
                {
                    Action::Wait
                } else {
                    drop(state);
                    let target = self.build_cache.pending_generation();
                    let mut state = self
                        .state
                        .lock()
                        .expect("cache lifecycle mutex should not be poisoned");
                    if matches!(state.activity, CacheActivity::Active { .. })
                        || state.flush.is_some()
                    {
                        Action::Wait
                    } else if let Some(target_generation) = target {
                        state.flush = Some(InFlightCacheFlush { target_generation });
                        Action::Start(target_generation)
                    } else {
                        if matches!(state.activity, CacheActivity::Idle) {
                            state.activity = CacheActivity::Ready;
                        }
                        Action::Done(CacheLifecycleOutcome::default())
                    }
                }
            };

            match action {
                Action::Start(target_generation) => {
                    self.notify_changed();
                    self.perform_flush(target_generation).await;
                }
                Action::Wait => {
                    if changed.changed().await.is_err() {
                        return CacheLifecycleOutcome {
                            diagnostic: Some("cache lifecycle stopped unexpectedly".to_string()),
                            ..CacheLifecycleOutcome::default()
                        };
                    }
                }
                Action::Done(mut outcome) => {
                    outcome
                        .infrastructure_log_events
                        .extend(self.build_cache.take_infrastructure_log_events());
                    self.notify_changed();
                    return outcome;
                }
            }
        }
    }

    async fn shutdown(self: &Arc<Self>) -> CacheLifecycleOutcome {
        let mut changed = self.changed.subscribe();
        loop {
            enum Action {
                Start(u64),
                Wait,
                Retry,
                Done(CacheLifecycleOutcome),
            }

            let action = {
                let mut state = self
                    .state
                    .lock()
                    .expect("cache lifecycle mutex should not be poisoned");
                match state.activity {
                    CacheActivity::Closed => {
                        Action::Done(state.shutdown_outcome.clone().unwrap_or_default())
                    }
                    CacheActivity::Ready | CacheActivity::Idle => {
                        state.activity = CacheActivity::ShuttingDown;
                        state.scheduled = None;
                        state.initial_store_deadline = None;
                        state.ordinary_deadline = None;
                        state.large_change_deadline = None;
                        Action::Retry
                    }
                    CacheActivity::Active { run_id } => {
                        state.activity = CacheActivity::ShuttingDown;
                        state.shutdown_run_id = Some(run_id);
                        state.scheduled = None;
                        state.initial_store_deadline = None;
                        state.ordinary_deadline = None;
                        state.large_change_deadline = None;
                        Action::Retry
                    }
                    CacheActivity::ShuttingDown => {
                        if state.shutdown_run_id.is_some() || state.flush.is_some() {
                            Action::Wait
                        } else {
                            let diagnostic = state.diagnostic.take();
                            drop(state);
                            let target = self.build_cache.pending_generation();
                            let mut state = self
                                .state
                                .lock()
                                .expect("cache lifecycle mutex should not be poisoned");
                            if state.shutdown_run_id.is_some() || state.flush.is_some() {
                                Action::Wait
                            } else if diagnostic.is_some() {
                                let outcome = CacheLifecycleOutcome {
                                    diagnostic,
                                    ..CacheLifecycleOutcome::default()
                                };
                                state.activity = CacheActivity::Closed;
                                state.shutdown_outcome = Some(outcome.clone());
                                Action::Done(outcome)
                            } else if let Some(target_generation) = target {
                                state.flush = Some(InFlightCacheFlush { target_generation });
                                Action::Start(target_generation)
                            } else {
                                let outcome = CacheLifecycleOutcome::default();
                                state.activity = CacheActivity::Closed;
                                state.shutdown_outcome = Some(outcome.clone());
                                Action::Done(outcome)
                            }
                        }
                    }
                }
            };
            match action {
                Action::Start(target_generation) => {
                    self.notify_changed();
                    self.perform_flush(target_generation).await;
                }
                Action::Wait => {
                    if changed.changed().await.is_err() {
                        return CacheLifecycleOutcome {
                            diagnostic: Some("cache lifecycle stopped unexpectedly".to_string()),
                            ..CacheLifecycleOutcome::default()
                        };
                    }
                }
                Action::Retry => {
                    self.notify_changed();
                }
                Action::Done(mut outcome) => {
                    outcome
                        .infrastructure_log_events
                        .extend(self.build_cache.take_infrastructure_log_events());
                    self.notify_changed();
                    return outcome;
                }
            }
        }
    }

    #[cfg(test)]
    async fn wait_for_idle_publication(&self) {
        let mut changed = self.changed.subscribe();
        loop {
            let (flush_in_flight, diagnostic) = {
                let state = self
                    .state
                    .lock()
                    .expect("cache lifecycle mutex should not be poisoned");
                (state.flush.is_some(), state.diagnostic.clone())
            };
            assert!(
                diagnostic.is_none(),
                "cache publication failed: {diagnostic:?}"
            );
            if self.build_cache.pending_generation().is_none() && !flush_in_flight {
                return;
            }
            changed
                .changed()
                .await
                .expect("cache lifecycle should remain observable");
        }
    }

    fn notify_changed(&self) {
        self.changed.send_modify(|version| {
            *version = version.wrapping_add(1);
        });
    }
}

#[derive(Debug)]
struct CacheRunActivity {
    lifecycle: Weak<CacheLifecycle>,
    run_id: u64,
    idle_reason: CacheIdleReason,
}

impl Drop for CacheRunActivity {
    fn drop(&mut self) {
        if let Some(lifecycle) = self.lifecycle.upgrade() {
            lifecycle.begin_idle(self.run_id, self.idle_reason);
        }
    }
}

impl Compiler {
    pub fn new(options: CompilerOptions) -> Self {
        let build_cache = BuildCache::new(options.cache.clone(), options.snapshot.clone());
        let cache_lifecycle = CacheLifecycle::new(build_cache.clone(), &options.cache);
        Self {
            options,
            build_cache,
            cache_lifecycle,
        }
    }

    pub fn options(&self) -> &CompilerOptions {
        &self.options
    }

    pub fn create_compilation(&self) -> Compilation {
        Compilation::new(
            self.options.clone(),
            UnpackResolver::new(self.options.resolve.clone()),
            self.build_cache.clone(),
        )
    }

    pub async fn run(&self) -> Result<Compilation> {
        Ok(self
            .run_until_finalize(CacheIdleReason::Ordinary)
            .await?
            .finish())
    }

    pub async fn run_until_finalize(
        &self,
        idle_reason: CacheIdleReason,
    ) -> Result<PendingCompilation> {
        self.build_cache
            .prepare_for_compilation(
                &self.options.context,
                &UnpackResolver::new(self.options.resolve.clone()),
            )
            .await?;
        let cache_activity = self.cache_lifecycle.end_idle(idle_reason)?;
        let result = async {
            let mut compilation = self.create_compilation();
            compilation.make().await?;
            compilation.seal();
            Ok(compilation)
        }
        .instrument(tracing::trace_span!("Compiler::run"))
        .await;
        if result.is_ok() {
            self.build_cache.store_build_dependencies();
            self.build_cache.on_compilation_completed();
        }
        self.build_cache.trace_work_counters();
        result.map(|mut compilation| {
            compilation.extend_infrastructure_log_events(
                self.build_cache.take_infrastructure_log_events(),
            );
            PendingCompilation {
                compilation: Some(compilation),
                cache_activity: Some(cache_activity),
            }
        })
    }

    pub fn flush_cache(&self) -> std::result::Result<(), String> {
        let span = tracing::trace_span!("Compiler::flush_cache");
        let _enter = span.enter();
        self.build_cache
            .flush_to_filesystem()
            .map_err(|error| error.to_string())
    }

    pub async fn settle_cache(&self) -> CacheLifecycleOutcome {
        self.cache_lifecycle.settle().await
    }

    pub async fn shutdown(&self) -> CacheLifecycleOutcome {
        self.cache_lifecycle.shutdown().await
    }

    #[cfg(test)]
    async fn wait_for_idle_cache_publication(&self) {
        self.cache_lifecycle.wait_for_idle_publication().await;
    }
}

fn default_resolve_options() -> ResolveOptions {
    let mut options = ResolveOptions::default();
    options.extensions = DEFAULT_EXTENSIONS
        .iter()
        .map(|extension| (*extension).to_string())
        .collect();
    options
}

fn normalize_context(context: PathBuf) -> PathBuf {
    if context.is_absolute() {
        context
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&context))
            .unwrap_or(context)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        path::Path,
        sync::{Arc, Barrier},
    };

    use super::*;
    use crate::{
        BuildDependency,
        build_cache::{CacheItemFamily, CacheItemWork},
        pack_file::{CodecRegistry, ModuleBuildRecordCodec, PackFile, ResolveRecordCodec},
    };

    #[tokio::test]
    async fn repeated_runs_reuse_memory_module_build_records_without_sharing_compilations()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(
            temp.path().join("index.js"),
            r#"
                import "./dep";
                export const result = "ok";
            "#,
        )?;
        write(temp.path().join("dep.js"), "export const value = 1;")?;

        let compiler = Compiler::new(CompilerOptions::new(
            temp.path(),
            vec![Entry::new("main", "./index")],
        ));

        let first = compiler.run().await?;
        let first_cache = compiler.build_cache.stats();
        assert_eq!(first_cache.module_entries, 2);
        assert_eq!(first_cache.module_hits, 0);
        assert_eq!(first_cache.module_misses, 2);
        assert_eq!(first_cache.resolve_entries, 2);
        assert_eq!(first_cache.resolve_hits, 0);
        assert_eq!(first_cache.resolve_misses, 2);
        let first_work = compiler.build_cache.work_counters();
        assert_eq!(
            first_work.for_family(CacheItemFamily::Resolve),
            CacheItemWork {
                hits: 0,
                misses: 2,
                stores: 2,
                restores: 0,
                evictions: 0,
            }
        );
        assert_eq!(
            first_work.for_family(CacheItemFamily::ModuleBuild),
            CacheItemWork {
                hits: 0,
                misses: 2,
                stores: 2,
                restores: 0,
                evictions: 0,
            }
        );

        let second = compiler.run().await?;
        let second_cache = compiler.build_cache.stats();
        assert_eq!(second_cache.module_entries, 2);
        assert_eq!(second_cache.module_hits, 2);
        assert_eq!(second_cache.module_misses, 2);
        assert_eq!(second_cache.resolve_entries, 2);
        assert_eq!(second_cache.resolve_hits, 2);
        assert_eq!(second_cache.resolve_misses, 2);
        let second_work = compiler.build_cache.work_counters();
        assert_eq!(
            second_work.for_family(CacheItemFamily::Resolve),
            CacheItemWork {
                hits: 2,
                misses: 2,
                stores: 2,
                restores: 0,
                evictions: 0,
            }
        );
        assert_eq!(
            second_work.for_family(CacheItemFamily::ModuleBuild),
            CacheItemWork {
                hits: 2,
                misses: 2,
                stores: 2,
                restores: 0,
                evictions: 0,
            }
        );

        assert_eq!(first.errors(), []);
        assert_eq!(second.errors(), []);
        assert_eq!(asset_sources(&first), asset_sources(&second));
        assert_eq!(first.module_graph(), second.module_graph());
        assert_ne!(
            first.module_graph().modules().as_ptr(),
            second.module_graph().modules().as_ptr()
        );
        assert_ne!(
            first.chunk_graph().chunks().as_ptr(),
            second.chunk_graph().chunks().as_ptr()
        );

        Ok(())
    }

    #[tokio::test]
    async fn hash_module_snapshot_strategy_invalidates_changed_source()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let entry = temp.path().join("index.js");
        write(&entry, "export const value = 'before';")?;

        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.snapshot.module = crate::SnapshotStrategy::hash();
        let compiler = Compiler::new(options);

        let first = compiler.run().await?;
        assert!(
            asset_sources(&first)
                .get("main.js")
                .expect("main asset should exist")
                .contains("before")
        );

        write(&entry, "export const value = 'after';")?;

        let second = compiler.run().await?;
        assert!(
            asset_sources(&second)
                .get("main.js")
                .expect("main asset should exist")
                .contains("after")
        );
        assert_ne!(
            first.module_graph().modules().as_ptr(),
            second.module_graph().modules().as_ptr()
        );
        assert_ne!(
            first.chunk_graph().chunks().as_ptr(),
            second.chunk_graph().chunks().as_ptr()
        );

        Ok(())
    }

    #[tokio::test]
    async fn filesystem_cache_restores_module_build_records_for_later_compiler_instances()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(
            temp.path().join("index.js"),
            r#"
                import "./dep";
                export const result = "ok";
            "#,
        )?;
        write(temp.path().join("dep.js"), "export const value = 1;")?;
        let cache_location = temp.path().join(".cache/unpack/default");

        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location.clone());
        options.cache.version = Some("test-version".to_string());

        let first_compiler = Compiler::new(options.clone());
        let first = first_compiler.run().await?;
        first_compiler.flush_cache()?;
        assert_eq!(first.errors(), []);
        assert!(PackFile::index_path(&cache_location).exists());
        assert!(!cache_location.join("container.json").exists());
        assert!(!cache_location.join("packs/modules.cbor").exists());

        let second_compiler = Compiler::new(options);
        assert_eq!(second_compiler.build_cache.stats().resolve_entries, 0);
        assert_eq!(second_compiler.build_cache.stats().module_entries, 0);

        let second = second_compiler.run().await?;
        let second_cache = second_compiler.build_cache.stats();
        assert_eq!(second_cache.resolve_hits, 2);
        assert_eq!(second_cache.module_hits, 2);
        assert_eq!(asset_sources(&first), asset_sources(&second));
        assert_ne!(
            first.module_graph().modules().as_ptr(),
            second.module_graph().modules().as_ptr(),
            "a later Compiler must assemble a fresh ModuleGraph"
        );
        let restored_work = second_compiler.build_cache.work_counters();
        assert_eq!(
            restored_work.for_family(CacheItemFamily::Resolve),
            CacheItemWork {
                hits: 2,
                misses: 0,
                stores: 0,
                restores: 2,
                evictions: 0,
            }
        );
        assert_eq!(
            restored_work.for_family(CacheItemFamily::ModuleBuild),
            CacheItemWork {
                hits: 2,
                misses: 0,
                stores: 0,
                restores: 2,
                evictions: 0,
            }
        );
        assert_eq!(
            restored_work.for_family(CacheItemFamily::CodeGeneration),
            CacheItemWork {
                hits: 2,
                misses: 0,
                stores: 0,
                restores: 2,
                evictions: 0,
            }
        );

        let third = second_compiler.run().await?;
        let repopulated_work = second_compiler.build_cache.work_counters();
        assert_eq!(
            repopulated_work.for_family(CacheItemFamily::Resolve),
            CacheItemWork {
                hits: 4,
                misses: 0,
                stores: 0,
                restores: 2,
                evictions: 0,
            }
        );
        assert_eq!(
            repopulated_work.for_family(CacheItemFamily::ModuleBuild),
            CacheItemWork {
                hits: 4,
                misses: 0,
                stores: 0,
                restores: 2,
                evictions: 0,
            }
        );
        assert_eq!(
            repopulated_work.for_family(CacheItemFamily::CodeGeneration),
            CacheItemWork {
                hits: 4,
                misses: 0,
                stores: 0,
                restores: 2,
                evictions: 0,
            }
        );
        assert_eq!(asset_sources(&second), asset_sources(&third));
        assert_ne!(
            second.module_graph().modules().as_ptr(),
            third.module_graph().modules().as_ptr()
        );
        assert_ne!(
            second.chunk_graph().chunks().as_ptr(),
            third.chunk_graph().chunks().as_ptr()
        );
        for restored_module in second.module_graph().modules() {
            let cached_record = second_compiler
                .build_cache
                .module_builds()
                .get(restored_module.identity(), None)
                .expect("restored Module Build Record should remain in Memory Cache");
            let rebuilt_module = third
                .module_graph()
                .modules()
                .iter()
                .find(|module| module.identity() == restored_module.identity())
                .expect("later ModuleGraph should contain the same Module Identity");

            assert!(
                !std::ptr::eq(restored_module, rebuilt_module),
                "each Compilation must create a distinct Module object"
            );
            assert!(Arc::ptr_eq(
                restored_module.built_content(),
                cached_record.built_content()
            ));
            assert!(Arc::ptr_eq(
                restored_module.built_content(),
                rebuilt_module.built_content()
            ));
        }

        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn compiler_owns_initial_idle_cache_publication()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(temp.path().join("index.js"), "export const value = 1;")?;
        let cache_temp = tempfile::tempdir()?;
        let cache_location = cache_temp.path().join("compiler-owned-idle");
        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location.clone());
        options.cache.idle_timeout = Some(60_000);
        options.cache.idle_timeout_for_initial_store = Some(20);

        let compiler = Compiler::new(options);
        compiler.run().await?;
        assert!(!PackFile::index_path(&cache_location).exists());

        tokio::time::advance(std::time::Duration::from_millis(19)).await;
        assert!(!PackFile::index_path(&cache_location).exists());
        tokio::time::advance(std::time::Duration::from_millis(1)).await;
        compiler.wait_for_idle_cache_publication().await;
        assert!(PackFile::index_path(&cache_location).exists());
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn cache_idle_begins_only_after_the_run_is_finalized()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(temp.path().join("index.js"), "export const value = 1;")?;
        let cache_temp = tempfile::tempdir()?;
        let cache_location = cache_temp.path().join("finalize-before-idle");
        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location.clone());
        options.cache.idle_timeout_for_initial_store = Some(0);

        let compiler = Compiler::new(options);
        let pending = compiler
            .run_until_finalize(CacheIdleReason::Ordinary)
            .await?;
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        assert!(!PackFile::index_path(&cache_location).exists());

        let compilation = pending.finish();
        assert_eq!(compilation.errors(), []);
        compiler.wait_for_idle_cache_publication().await;
        assert!(PackFile::index_path(&cache_location).exists());
        Ok(())
    }

    #[tokio::test]
    async fn settling_idle_cache_drains_pending_work_and_keeps_compiler_reusable()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(temp.path().join("index.js"), "export const value = 1;")?;
        let cache_temp = tempfile::tempdir()?;
        let cache_location = cache_temp.path().join("settled-watch-cache");
        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location.clone());
        options.cache.idle_timeout_for_initial_store = Some(60_000);

        let compiler = Compiler::new(options);
        compiler.run().await?;
        assert!(!PackFile::index_path(&cache_location).exists());

        let outcome = compiler.settle_cache().await;
        assert_eq!(outcome.diagnostic(), None);
        assert!(PackFile::index_path(&cache_location).exists());
        assert_eq!(compiler.run().await?.errors(), []);
        Ok(())
    }

    #[tokio::test]
    async fn shutdown_drains_pending_cache_work_is_idempotent_and_prevents_new_runs()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(temp.path().join("index.js"), "export const value = 1;")?;
        let cache_temp = tempfile::tempdir()?;
        let cache_location = cache_temp.path().join("shutdown-cache");
        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location.clone());
        options.cache.idle_timeout_for_initial_store = Some(60_000);

        let compiler = Compiler::new(options);
        compiler.run().await?;
        assert!(!PackFile::index_path(&cache_location).exists());

        assert_eq!(compiler.shutdown().await.diagnostic(), None);
        assert!(PackFile::index_path(&cache_location).exists());
        assert_eq!(compiler.shutdown().await.diagnostic(), None);
        assert!(matches!(
            compiler.run().await,
            Err(crate::Error::CompilerClosed)
        ));
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn repeated_run_keeps_the_earliest_initial_store_deadline()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let entry = temp.path().join("index.js");
        write(&entry, "export const value = 'before';")?;
        let cache_temp = tempfile::tempdir()?;
        let cache_location = cache_temp.path().join("initial-deadline");
        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location.clone());
        options.cache.idle_timeout = Some(500);
        options.cache.idle_timeout_for_initial_store = Some(200);

        let compiler = Compiler::new(options);
        compiler.run().await?;
        tokio::time::advance(std::time::Duration::from_millis(150)).await;
        write(&entry, "export const value = 'after';")?;
        compiler.run().await?;

        tokio::time::advance(std::time::Duration::from_millis(49)).await;
        assert!(!PackFile::index_path(&cache_location).exists());
        tokio::time::advance(std::time::Duration::from_millis(1)).await;
        compiler.wait_for_idle_cache_publication().await;
        assert!(PackFile::index_path(&cache_location).exists());
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn store_during_publication_remains_dirty_for_a_later_revision()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let entry = temp.path().join("index.js");
        write(&entry, "export const value = 'before';")?;
        let cache_temp = tempfile::tempdir()?;
        let cache_location = cache_temp.path().join("generation-race");
        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location.clone());
        options.cache.idle_timeout_for_initial_store = Some(60_000);

        let compiler = Compiler::new(options);
        compiler.run().await?;
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        compiler
            .build_cache
            .install_publish_barrier(entered.clone(), release.clone());

        let settling_compiler = compiler.clone();
        let settling = tokio::spawn(async move { settling_compiler.settle_cache().await });
        tokio::task::yield_now().await;
        entered.wait();

        write(&entry, "export const value = 'after';")?;
        let running_compiler = compiler.clone();
        let running = tokio::spawn(async move { running_compiler.run().await });
        tokio::task::yield_now().await;
        release.wait();

        assert_eq!(settling.await?.diagnostic(), None);
        assert_eq!(running.await??.errors(), []);
        assert_eq!(compiler.settle_cache().await.diagnostic(), None);
        let registry = CodecRegistry::new()
            .with_resolve_record(ResolveRecordCodec::current())
            .with_module_build_record(ModuleBuildRecordCodec::current());
        assert_eq!(PackFile::open(&cache_location, registry).revision(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn build_dependencies_are_stored_before_finalize_and_begin_idle()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(temp.path().join("index.js"), "export const value = 1;")?;
        let config = temp.path().join("build.config.js");
        write(&config, "export default 'before';")?;
        let cache_temp = tempfile::tempdir()?;
        let cache_location = cache_temp.path().join("build-dependency-order");
        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location);
        options.cache.idle_timeout_for_initial_store = Some(60_000);
        options.cache.build_dependencies = vec![BuildDependency {
            name: "config".to_string(),
            requests: vec![config.display().to_string()],
        }];

        let compiler = Compiler::new(options.clone());
        let pending = compiler
            .run_until_finalize(CacheIdleReason::Ordinary)
            .await?;
        write(&config, "export default 'after';")?;
        pending.finish();
        assert_eq!(compiler.settle_cache().await.diagnostic(), None);

        let second = Compiler::new(options);
        second.run().await?;
        assert_eq!(
            second
                .build_cache
                .work_counters()
                .for_family(CacheItemFamily::ModuleBuild),
            CacheItemWork {
                hits: 0,
                misses: 1,
                stores: 1,
                restores: 0,
                evictions: 0,
            }
        );
        Ok(())
    }

    #[tokio::test]
    async fn aged_filesystem_entries_restore_from_pack_without_reusing_a_compilation_graph()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let entry = temp.path().join("index.js");
        write(
            &entry,
            r#"
                import { value } from "./dep";
                export const result = value;
            "#,
        )?;
        write(
            temp.path().join("dep.js"),
            "export const value = 'from-stable-dependency';",
        )?;
        let cache_location = temp.path().join(".cache/unpack/generations");

        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location);
        options.cache.max_memory_generations = Some(1);
        options.snapshot.module = crate::SnapshotStrategy::hash();
        let compiler = Compiler::new(options);

        let first = compiler.run().await?;
        compiler.flush_cache()?;
        assert!(
            asset_sources(&first)
                .get("main.js")
                .expect("main asset should exist")
                .contains("from-stable-dependency")
        );

        write(&entry, "export const result = 'dependency-unused';")?;
        let second = compiler.run().await?;
        assert!(
            asset_sources(&second)
                .get("main.js")
                .expect("main asset should exist")
                .contains("dependency-unused")
        );
        assert_eq!(
            compiler
                .build_cache
                .work_counters()
                .for_family(CacheItemFamily::ModuleBuild)
                .evictions,
            1
        );

        write(
            &entry,
            r#"
                import { value } from "./dep";
                export const result = value;
            "#,
        )?;
        let third = compiler.run().await?;
        let work = compiler.build_cache.work_counters();

        assert!(
            asset_sources(&third)
                .get("main.js")
                .expect("main asset should exist")
                .contains("from-stable-dependency")
        );
        assert_eq!(work.for_family(CacheItemFamily::Resolve).restores, 1);
        assert_eq!(work.for_family(CacheItemFamily::ModuleBuild).restores, 1);
        assert_ne!(
            second.module_graph().modules().as_ptr(),
            third.module_graph().modules().as_ptr()
        );
        assert_ne!(
            second.chunk_graph().chunks().as_ptr(),
            third.chunk_graph().chunks().as_ptr()
        );

        Ok(())
    }

    #[tokio::test]
    async fn zero_filesystem_memory_generations_read_directly_from_pack()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(
            temp.path().join("index.js"),
            r#"
                import "./dep";
                export const result = "ok";
            "#,
        )?;
        write(temp.path().join("dep.js"), "export const value = 1;")?;

        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(temp.path().join(".cache/unpack/no-memory"));
        options.cache.max_memory_generations = Some(0);
        let compiler = Compiler::new(options);

        let first = compiler.run().await?;
        compiler.flush_cache()?;
        let second = compiler.run().await?;
        let work = compiler.build_cache.work_counters();

        assert_eq!(asset_sources(&first), asset_sources(&second));
        assert_eq!(
            work.for_family(CacheItemFamily::Resolve),
            CacheItemWork {
                hits: 2,
                misses: 2,
                stores: 2,
                restores: 0,
                evictions: 0,
            }
        );
        assert_eq!(
            work.for_family(CacheItemFamily::ModuleBuild),
            CacheItemWork {
                hits: 2,
                misses: 2,
                stores: 2,
                restores: 0,
                evictions: 0,
            }
        );
        assert_eq!(compiler.build_cache.stats().module_entries, 0);
        assert_ne!(
            first.module_graph().modules().as_ptr(),
            second.module_graph().modules().as_ptr()
        );
        assert_ne!(
            first.chunk_graph().chunks().as_ptr(),
            second.chunk_graph().chunks().as_ptr()
        );

        Ok(())
    }

    #[tokio::test]
    async fn filesystem_cache_readonly_restores_but_skips_persistent_updates()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(
            temp.path().join("index.js"),
            r#"
                import { value } from "./dep";
                export const result = value;
            "#,
        )?;
        write(temp.path().join("dep.js"), "export const value = 'before';")?;
        let cache_location = temp.path().join(".cache/unpack/default");

        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location.clone());
        options.snapshot.module = crate::SnapshotStrategy::hash();

        let first_compiler = Compiler::new(options.clone());
        first_compiler.run().await?;
        first_compiler.flush_cache()?;
        let cache_before = directory_snapshot(&cache_location)?;

        write(temp.path().join("dep.js"), "export const value = 'after';")?;

        let mut readonly_options = options;
        readonly_options.cache.readonly = true;
        readonly_options.cache.max_age = std::time::Duration::ZERO;
        let readonly_compiler = Compiler::new(readonly_options);
        assert_eq!(readonly_compiler.build_cache.stats().resolve_entries, 0);
        assert_eq!(readonly_compiler.build_cache.stats().module_entries, 0);

        let second = readonly_compiler.run().await?;
        let readonly_cache = readonly_compiler.build_cache.stats();
        assert_eq!(readonly_cache.resolve_entries, 2);
        assert_eq!(readonly_cache.module_entries, 2);
        readonly_compiler.flush_cache()?;
        assert!(
            asset_sources(&second)
                .get("main.js")
                .expect("main asset should exist")
                .contains("after")
        );
        assert_eq!(directory_snapshot(&cache_location)?, cache_before);

        Ok(())
    }

    #[tokio::test]
    async fn filesystem_cache_readonly_does_not_create_persistent_cache()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(temp.path().join("index.js"), "export const value = 1;")?;
        let cache_location = temp.path().join(".cache/unpack/default");

        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location.clone());
        options.cache.readonly = true;

        let compiler = Compiler::new(options);
        compiler.run().await?;
        compiler.flush_cache()?;

        assert!(!cache_location.exists());

        Ok(())
    }

    #[tokio::test]
    async fn filesystem_cache_rejects_invalid_build_dependency_snapshots()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(temp.path().join("index.js"), "export const value = 1;")?;
        let config = temp.path().join("config.js");
        write(&config, "export default 'before';")?;
        let cache_location = temp.path().join(".cache/unpack/default");

        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location);
        options.cache.build_dependencies = vec![crate::BuildDependency {
            name: "config".to_string(),
            requests: vec![config.display().to_string()],
        }];

        let first_compiler = Compiler::new(options.clone());
        first_compiler.run().await?;
        first_compiler.flush_cache()?;
        let warm_compiler = Compiler::new(options.clone());
        warm_compiler.run().await?;
        assert_eq!(
            warm_compiler
                .build_cache
                .work_counters()
                .for_family(CacheItemFamily::ModuleBuild),
            CacheItemWork {
                hits: 1,
                misses: 0,
                stores: 0,
                restores: 1,
                evictions: 0,
            }
        );

        write(&config, "export default 'after';")?;
        let cold_compiler = Compiler::new(options);
        cold_compiler.run().await?;
        assert_eq!(
            cold_compiler
                .build_cache
                .work_counters()
                .for_family(CacheItemFamily::ModuleBuild),
            CacheItemWork {
                hits: 0,
                misses: 1,
                stores: 1,
                restores: 0,
                evictions: 0,
            }
        );

        Ok(())
    }

    #[tokio::test]
    async fn filesystem_cache_rechecks_missing_resolve_candidates()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(
            temp.path().join("index.js"),
            r#"
                import { value } from "./dep";
                export const result = value;
            "#,
        )?;
        write(temp.path().join("dep.js"), "export const value = 'js';")?;
        let cache_location = temp.path().join(".cache/unpack/default");

        let mut options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        options.cache = CacheOptions::filesystem();
        options.cache.cache_location = Some(cache_location);

        let first_compiler = Compiler::new(options.clone());
        let first = first_compiler.run().await?;
        first_compiler.flush_cache()?;
        assert!(
            asset_sources(&first)
                .get("main.js")
                .expect("main asset should exist")
                .contains("'js'")
        );

        write(temp.path().join("dep.ts"), "export const value = 'ts';")?;

        let second_compiler = Compiler::new(options);
        let second = second_compiler.run().await?;
        assert!(
            asset_sources(&second)
                .get("main.js")
                .expect("main asset should exist")
                .contains("'ts'")
        );

        Ok(())
    }

    fn write(path: impl AsRef<Path>, source: &str) -> std::io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, source)
    }

    fn asset_sources(compilation: &Compilation) -> BTreeMap<String, String> {
        compilation
            .assets()
            .iter()
            .map(|asset| (asset.filename.clone(), asset.source.clone()))
            .collect()
    }

    fn directory_snapshot(root: &Path) -> std::io::Result<BTreeMap<PathBuf, Vec<u8>>> {
        fn visit(
            root: &Path,
            directory: &Path,
            files: &mut BTreeMap<PathBuf, Vec<u8>>,
        ) -> std::io::Result<()> {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    visit(root, &path, files)?;
                } else {
                    files.insert(
                        path.strip_prefix(root).unwrap_or(&path).to_path_buf(),
                        fs::read(path)?,
                    );
                }
            }
            Ok(())
        }

        let mut files = BTreeMap::new();
        visit(root, root, &mut files)?;
        Ok(files)
    }
}
