// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/Cache.js

//! Compiler-owned webpack Cache responsibility.
//! It wires typed Cache Facades to Cache Layers and owns preparation and publication lifecycle.

mod cache_items;
mod cache_layers;
mod memory_cache_plugin;
mod memory_with_gc_cache_plugin;
mod options;
pub(crate) mod pack_file;
mod pack_file_cache_strategy;

pub use options::{BuildDependency, CacheCompression, CacheKind, CacheOptions};

pub(crate) use cache_items::{ModuleBuildRecord, ResolveRecord, ResolveRequest};
pub(crate) use cache_layers::{CacheItemFamily, CacheItemWork, CacheWorkCounters};

#[cfg(test)]
#[path = "cache/cache_tests.rs"]
mod tests;

use std::{
    collections::BTreeSet,
    fmt, fs, io,
    marker::PhantomData,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use crate::{
    InfrastructureLogEvent, InfrastructureLogLevel, ModuleIdentity, SnapshotOptions,
    SnapshotStrategy, UnpackResolver,
    cache::{
        cache_layers::{CacheGet, CacheLayers},
        pack_file::{AccessStamp, PackFileGuardDto, SnapshotDto},
        pack_file_cache_strategy::PersistentCachePreparation,
    },
    cache_facade::{
        ASSET_RENDER_CACHE_NAMESPACE, CODE_GENERATION_CACHE_NAMESPACE, CacheAddress, CacheETag,
        CacheFacade, CacheNamespace, MODULE_BUILD_CACHE_NAMESPACE, ModuleBuildCache,
        NormalModuleFactoryCache, RESOLVE_CACHE_NAMESPACE,
    },
    code_generation_record::CodeGenerationRecord,
    rendered_source::RenderedSource,
    snapshot::{FileSystemInfo, Snapshot, SnapshotCache},
};

const CACHE_PROFILE_LOGGER: &str = "unpack.Cache.Profile";
const CACHE_WRITER_LOGGER: &str = "unpack.Cache.Writer";

#[derive(Debug)]
pub(crate) struct CacheDiagnostics {
    profile: bool,
    events: Mutex<Vec<InfrastructureLogEvent>>,
}

impl CacheDiagnostics {
    fn new(profile: bool) -> Self {
        Self {
            profile,
            events: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn profile(&self, message: impl Into<String>) {
        if self.profile {
            self.events
                .lock()
                .expect("cache diagnostics mutex should not be poisoned")
                .push(InfrastructureLogEvent::new(
                    InfrastructureLogLevel::Log,
                    CACHE_PROFILE_LOGGER,
                    message,
                ));
        }
    }

    pub(super) fn profile_enabled(&self) -> bool {
        self.profile
    }

    pub(super) fn warn(&self, message: impl Into<String>) {
        self.events
            .lock()
            .expect("cache diagnostics mutex should not be poisoned")
            .push(InfrastructureLogEvent::new(
                InfrastructureLogLevel::Warn,
                CACHE_WRITER_LOGGER,
                message,
            ));
    }

    fn drain(&self) -> Vec<InfrastructureLogEvent> {
        std::mem::take(
            &mut *self
                .events
                .lock()
                .expect("cache diagnostics mutex should not be poisoned"),
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Cache {
    options: CacheOptions,
    build_dependency_snapshot_strategy: SnapshotStrategy,
    resolve_build_dependency_snapshot_strategy: SnapshotStrategy,
    build_dependency_file_system_info: FileSystemInfo,
    diagnostics: Arc<CacheDiagnostics>,
    clock: Arc<dyn CacheClock>,
    inner: Arc<CacheInner>,
}

#[derive(Debug)]
struct CacheInner {
    // Operations that need both locks must acquire `cache` before `publication`.
    cache: Mutex<CacheLayers>,
    dirty_generation: AtomicU64,
    published_generation: AtomicU64,
    initial_store_pending: AtomicBool,
    publication: Mutex<CachePublicationState>,
}

#[derive(Debug)]
struct CachePublicationState {
    persistent_guard: Option<PackFileGuardDto>,
    persistent_guard_error: Option<String>,
    #[cfg(test)]
    publish_barrier: Option<PublishBarrier>,
}

impl CachePublicationState {
    fn wait_on_publish_barrier(&mut self) {
        #[cfg(test)]
        if let Some(barrier) = self.publish_barrier.take() {
            barrier.entered.wait();
            barrier.release.wait();
        }
    }
}

trait CacheClock: fmt::Debug + Send + Sync {
    fn now(&self) -> AccessStamp;
}

#[derive(Debug)]
struct SystemCacheClock;

impl CacheClock for SystemCacheClock {
    fn now(&self) -> AccessStamp {
        AccessStamp::now()
    }
}

#[cfg(test)]
#[derive(Debug)]
struct PublishBarrier {
    entered: Arc<std::sync::Barrier>,
    release: Arc<std::sync::Barrier>,
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct RestoreBarrier {
    pub(super) entered: Arc<std::sync::Barrier>,
    pub(super) release: Arc<std::sync::Barrier>,
}

#[cfg(test)]
#[derive(Debug)]
struct ManualCacheClock {
    now_millis: AtomicU64,
    calls: AtomicU64,
}

#[cfg(test)]
impl ManualCacheClock {
    fn at_millis(now_millis: u64) -> Self {
        Self {
            now_millis: AtomicU64::new(now_millis),
            calls: AtomicU64::new(0),
        }
    }

    fn set_millis(&self, now_millis: u64) {
        self.now_millis.store(now_millis, Ordering::SeqCst);
    }

    fn calls(&self) -> u64 {
        self.calls.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
impl CacheClock for ManualCacheClock {
    fn now(&self) -> AccessStamp {
        self.calls.fetch_add(1, Ordering::SeqCst);
        AccessStamp::from_millis(self.now_millis.load(Ordering::SeqCst))
    }
}

impl CacheInner {
    fn new(options: &CacheOptions, diagnostics: Arc<CacheDiagnostics>) -> Self {
        let cache = CacheLayers::from_options(options, diagnostics);
        let initial_store_pending = !cache.has_persistent_publication();
        Self {
            cache: Mutex::new(cache),
            dirty_generation: AtomicU64::new(0),
            published_generation: AtomicU64::new(0),
            initial_store_pending: AtomicBool::new(initial_store_pending),
            publication: Mutex::new(CachePublicationState {
                persistent_guard: None,
                persistent_guard_error: None,
                #[cfg(test)]
                publish_barrier: None,
            }),
        }
    }

    fn mark_dirty(&self) {
        self.dirty_generation
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |generation| {
                Some(generation.saturating_add(1))
            })
            .expect("cache generation update should always succeed");
    }
}
impl Cache {
    pub(crate) fn new(options: CacheOptions, snapshot_options: SnapshotOptions) -> Self {
        Self::new_with_clock_inner(options, snapshot_options, Arc::new(SystemCacheClock))
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.options.kind != CacheKind::Disabled
    }

    pub(crate) fn get<V>(
        &self,
        family: CacheItemFamily,
        address: &CacheAddress,
        etag: Option<&CacheETag>,
    ) -> Option<Arc<V>>
    where
        V: Send + Sync + 'static,
    {
        if !self.is_enabled() {
            return None;
        }
        let stamp = if self.is_writable_filesystem_cache() {
            self.clock.now()
        } else {
            AccessStamp::from_millis(0)
        };
        let mut result = {
            let mut layers = self
                .inner
                .cache
                .lock()
                .expect("cache layers mutex should not be poisoned");
            let result = layers.begin_get(family, address, etag, stamp);
            if result.persistent_access_changed() {
                self.inner.mark_dirty();
            }
            result
        };
        loop {
            match result {
                CacheGet::Ready { value, .. } => return value,
                CacheGet::Deferred(plan) => {
                    let restored = plan.restore.restore();
                    result = {
                        let mut layers = self
                            .inner
                            .cache
                            .lock()
                            .expect("cache layers mutex should not be poisoned");
                        let result =
                            layers.finish_restore(family, address, etag, stamp, plan, restored);
                        if result.persistent_access_changed() {
                            self.inner.mark_dirty();
                        }
                        result
                    };
                }
            }
        }
    }

    pub(crate) fn store<V>(
        &self,
        family: CacheItemFamily,
        address: CacheAddress,
        etag: Option<CacheETag>,
        value: V,
    ) where
        V: Send + Sync + 'static,
    {
        if !self.is_enabled() {
            return;
        }
        let mut layers = self
            .inner
            .cache
            .lock()
            .expect("cache layers mutex should not be poisoned");
        if layers.store(family, address, etag, value) {
            self.inner.mark_dirty();
        }
    }

    #[cfg(test)]
    pub(crate) fn evict_memory(&self, family: CacheItemFamily, address: &CacheAddress) {
        self.inner
            .cache
            .lock()
            .expect("cache layers mutex should not be poisoned")
            .evict_memory(family, address);
    }

    #[cfg(test)]
    fn new_with_clock<C>(
        options: CacheOptions,
        snapshot_options: SnapshotOptions,
        clock: Arc<C>,
    ) -> Self
    where
        C: CacheClock + 'static,
    {
        Self::new_with_clock_inner(options, snapshot_options, clock)
    }

    fn new_with_clock_inner(
        options: CacheOptions,
        snapshot_options: SnapshotOptions,
        clock: Arc<dyn CacheClock>,
    ) -> Self {
        let build_dependency_snapshot_strategy = snapshot_options.build_dependencies;
        let resolve_build_dependency_snapshot_strategy =
            snapshot_options.resolve_build_dependencies;
        let build_dependency_file_system_info = FileSystemInfo::for_build_dependencies();
        let diagnostics = Arc::new(CacheDiagnostics::new(options.profile));
        diagnostics.profile(
            "restore items=0; deserialization items=0; contract=trusted-local,linux-supported,single-writer coordination=none",
        );
        let inner = CacheInner::new(&options, diagnostics.clone());
        Self {
            options,
            build_dependency_snapshot_strategy,
            resolve_build_dependency_snapshot_strategy,
            build_dependency_file_system_info,
            diagnostics,
            clock,
            inner: Arc::new(inner),
        }
    }

    pub(crate) fn normal_module_factory(&self) -> NormalModuleFactoryCache {
        self.facade(RESOLVE_CACHE_NAMESPACE, CacheItemFamily::Resolve)
    }

    pub(crate) fn take_infrastructure_log_events(&self) -> Vec<InfrastructureLogEvent> {
        self.diagnostics.drain()
    }

    pub(crate) fn module_builds(&self) -> ModuleBuildCache {
        self.facade(MODULE_BUILD_CACHE_NAMESPACE, CacheItemFamily::ModuleBuild)
    }

    pub(crate) fn code_generations(&self) -> CacheFacade<ModuleIdentity, CodeGenerationRecord> {
        self.facade(
            CODE_GENERATION_CACHE_NAMESPACE,
            CacheItemFamily::CodeGeneration,
        )
    }

    pub(crate) fn asset_renders<K>(&self) -> CacheFacade<K, RenderedSource> {
        self.facade(ASSET_RENDER_CACHE_NAMESPACE, CacheItemFamily::AssetRender)
    }

    fn facade<K, V>(
        &self,
        namespace: CacheNamespace,
        family: CacheItemFamily,
    ) -> CacheFacade<K, V> {
        CacheFacade {
            cache: self.clone(),
            namespace,
            family,
            marker: PhantomData,
        }
    }

    pub(crate) async fn prepare_for_compilation(
        &self,
        context: &Path,
        resolver: &UnpackResolver,
    ) -> crate::Result<()> {
        if self.options.kind != CacheKind::Filesystem {
            return Ok(());
        }
        let requests = self
            .options
            .build_dependencies
            .iter()
            .flat_map(|dependency| dependency.requests.iter().cloned())
            .collect::<BTreeSet<_>>();
        let mut build_inputs = self
            .options
            .automatic_build_dependencies
            .iter()
            .map(|path| fs::canonicalize(path).unwrap_or_else(|_| path.clone()))
            .collect::<BTreeSet<_>>();
        let automatic_build_inputs = build_inputs.clone();
        let mut resolved_build_inputs = BTreeSet::new();
        let mut resolution_files = BTreeSet::new();
        let mut resolution_contexts = BTreeSet::new();
        let mut resolution_missing = BTreeSet::new();
        for request in requests {
            let resolved = resolver
                .resolve_with_dependencies(context, &request)
                .await?;
            let resource = resolved.resource.path;
            build_inputs.insert(resource.clone());
            resolved_build_inputs.insert(resource.clone());
            resolution_files.insert(resource);
            resolution_files.extend(resolved.file_dependencies);
            resolution_contexts.extend(resolved.context_dependencies);
            resolution_missing.extend(resolved.missing_dependencies);
        }
        let automatic_only = resolution_files.is_empty() && build_inputs == automatic_build_inputs;
        let build_dependency_snapshot_strategy = if automatic_only {
            SnapshotStrategy::timestamp()
        } else {
            self.build_dependency_snapshot_strategy
        };
        let build_dependencies = self
            .build_dependency_file_system_info
            .create_snapshot_sync(
                build_inputs.iter().cloned(),
                build_dependency_snapshot_strategy,
            )?;
        let resolve_build_dependencies = self
            .build_dependency_file_system_info
            .create_resolve_snapshot_with_cache(
                resolution_files,
                resolution_contexts,
                resolution_missing,
                self.resolve_build_dependency_snapshot_strategy,
                &SnapshotCache::default(),
            )
            .await?;
        let guard = PackFileGuardDto {
            version: self.cache_version().into_bytes(),
            build_dependencies: SnapshotDto::try_from(&build_dependencies)
                .expect("fresh Build Dependency Snapshot should encode"),
            resolve_build_dependencies: SnapshotDto::try_from(&resolve_build_dependencies)
                .expect("fresh Resolve Build Dependency Snapshot should encode"),
        };
        let mut cache = self
            .inner
            .cache
            .lock()
            .expect("build cache data mutex should not be poisoned");
        let mut publication = self
            .inner
            .publication
            .lock()
            .expect("build cache publication mutex should not be poisoned");
        let build_validation_strategy = if !build_inputs.is_empty()
            && build_inputs
                .iter()
                .all(|path| automatic_build_inputs.contains(path))
        {
            SnapshotStrategy::timestamp()
        } else {
            self.build_dependency_snapshot_strategy
        };
        let previous_build_inputs_are_valid = publication
            .persistent_guard
            .as_ref()
            .and_then(|previous| Snapshot::try_from(previous.build_dependencies.clone()).ok())
            .is_some_and(|snapshot| {
                snapshot.has_exact_paths(build_inputs.iter().cloned())
                    && self
                        .build_dependency_file_system_info
                        .is_snapshot_valid_sync(&snapshot, build_validation_strategy)
            });
        if publication.persistent_guard.is_some() && !previous_build_inputs_are_valid {
            cache.clear();
        }
        let guard_changed = cache.prepare_persistent(PersistentCachePreparation {
            guard: &guard,
            build_inputs: &build_inputs,
            resolved_build_inputs: &resolved_build_inputs,
            automatic_build_inputs: &automatic_build_inputs,
            file_system_info: &self.build_dependency_file_system_info,
            build_dependency_snapshot_strategy: self.build_dependency_snapshot_strategy,
            resolve_build_dependency_snapshot_strategy: self
                .resolve_build_dependency_snapshot_strategy,
        });
        publication.persistent_guard = Some(guard);
        if guard_changed {
            self.inner.mark_dirty();
        }
        Ok(())
    }

    pub(crate) fn store_build_dependencies(&self) {
        if !self.is_writable_filesystem_cache() {
            return;
        }
        // `prepare_for_compilation` records the resolved Build Dependency guard before work begins.
    }

    pub(crate) fn pending_generation(&self) -> Option<u64> {
        if !self.is_writable_filesystem_cache() {
            return None;
        }
        let published_generation = self.inner.published_generation.load(Ordering::Acquire);
        // Read dirty second so a concurrent store can only cause a redundant
        // publication, never hide a generation that still needs publishing.
        let dirty_generation = self.inner.dirty_generation.load(Ordering::Acquire);
        (dirty_generation > published_generation).then_some(dirty_generation)
    }

    pub(crate) fn initial_store_pending(&self) -> bool {
        self.inner.initial_store_pending.load(Ordering::Acquire)
    }

    pub(crate) fn publish_generation(&self, target_generation: u64) -> io::Result<()> {
        if !self.is_writable_filesystem_cache() {
            return Ok(());
        }
        let mut cache = self
            .inner
            .cache
            .lock()
            .expect("build cache data mutex should not be poisoned");
        if target_generation <= self.inner.published_generation.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut publication = self
            .inner
            .publication
            .lock()
            .expect("build cache publication mutex should not be poisoned");
        publication.wait_on_publish_barrier();
        if let Some(error) = &publication.persistent_guard_error {
            return Err(io::Error::new(io::ErrorKind::InvalidData, error.clone()));
        }
        let guard = publication
            .persistent_guard
            .clone()
            .unwrap_or_else(|| PackFileGuardDto {
                version: self.cache_version().into_bytes(),
                build_dependencies: SnapshotDto {
                    entries: Vec::new(),
                },
                resolve_build_dependencies: SnapshotDto {
                    entries: Vec::new(),
                },
            });
        let stamp = self.clock.now();
        cache.publish_persistent(guard, stamp, self.options.max_age)?;
        self.inner
            .published_generation
            .fetch_max(target_generation, Ordering::AcqRel);
        self.inner
            .initial_store_pending
            .store(false, Ordering::Release);
        Ok(())
    }

    fn is_writable_filesystem_cache(&self) -> bool {
        self.options.kind == CacheKind::Filesystem && !self.options.readonly
    }

    #[cfg(test)]
    pub(crate) fn install_publish_barrier(
        &self,
        entered: Arc<std::sync::Barrier>,
        release: Arc<std::sync::Barrier>,
    ) {
        self.inner
            .publication
            .lock()
            .expect("build cache publication mutex should not be poisoned")
            .publish_barrier = Some(PublishBarrier { entered, release });
    }

    #[cfg(test)]
    fn install_restore_barrier(
        &self,
        entered: Arc<std::sync::Barrier>,
        release: Arc<std::sync::Barrier>,
    ) {
        self.inner
            .cache
            .lock()
            .expect("build cache data mutex should not be poisoned")
            .install_restore_barrier(RestoreBarrier { entered, release });
    }

    pub(crate) fn flush_to_filesystem(&self) -> io::Result<()> {
        self.store_build_dependencies();
        let Some(target_generation) = self.pending_generation() else {
            return Ok(());
        };
        self.publish_generation(target_generation)
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> CacheStats {
        let cache = self
            .inner
            .cache
            .lock()
            .expect("build cache data mutex should not be poisoned");
        let work = cache.work_counters();
        let resolve: CacheItemWork = work.for_family(CacheItemFamily::Resolve);
        let module = work.for_family(CacheItemFamily::ModuleBuild);
        CacheStats {
            resolve_entries: cache.entry_count(CacheItemFamily::Resolve),
            resolve_hits: resolve.hits,
            resolve_misses: resolve.misses,
            module_entries: cache.entry_count(CacheItemFamily::ModuleBuild),
            module_hits: module.hits,
            module_misses: module.misses,
        }
    }

    pub(crate) fn work_counters(&self) -> CacheWorkCounters {
        self.inner
            .cache
            .lock()
            .expect("build cache data mutex should not be poisoned")
            .work_counters()
    }

    pub(crate) fn trace_work_counters(&self) {
        let work = self.work_counters();
        let resolve: CacheItemWork = work.for_family(CacheItemFamily::Resolve);
        let module = work.for_family(CacheItemFamily::ModuleBuild);
        let code_generation = work.for_family(CacheItemFamily::CodeGeneration);
        let asset_render = work.for_family(CacheItemFamily::AssetRender);
        tracing::info!(
            target: "unpack_core::cache_work",
            resolve_hits = resolve.hits,
            resolve_misses = resolve.misses,
            resolve_stores = resolve.stores,
            resolve_restores = resolve.restores,
            resolve_evictions = resolve.evictions,
            module_hits = module.hits,
            module_misses = module.misses,
            module_stores = module.stores,
            module_restores = module.restores,
            module_evictions = module.evictions,
            code_generation_hits = code_generation.hits,
            code_generation_misses = code_generation.misses,
            code_generation_stores = code_generation.stores,
            code_generation_restores = code_generation.restores,
            code_generation_evictions = code_generation.evictions,
            asset_render_hits = asset_render.hits,
            asset_render_misses = asset_render.misses,
            asset_render_stores = asset_render.stores,
            asset_render_restores = asset_render.restores,
            asset_render_evictions = asset_render.evictions,
            "cache_work"
        );
    }

    pub(crate) fn on_compilation_completed(&self) {
        self.inner
            .cache
            .lock()
            .expect("build cache data mutex should not be poisoned")
            .on_compilation_completed();
    }

    fn cache_version(&self) -> String {
        self.options.version.clone().unwrap_or_default()
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CacheStats {
    pub resolve_entries: usize,
    pub resolve_hits: usize,
    pub resolve_misses: usize,
    pub module_entries: usize,
    pub module_hits: usize,
    pub module_misses: usize,
}
