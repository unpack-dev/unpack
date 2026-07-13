use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{
    ModuleIdentity,
    cache::{ModuleBuildRecord, ResolveRecord, ResolveRequest},
    cache_facade::CacheETag,
};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WatchChangeSet {
    pub modified_files: FxHashSet<PathBuf>,
    pub removed_files: FxHashSet<PathBuf>,
    pub changed_contexts: FxHashSet<PathBuf>,
}

impl WatchChangeSet {
    pub(crate) fn affects_path(&self, path: &Path, path_is_context: bool) -> bool {
        self.modified_files.contains(path)
            || self.removed_files.contains(path)
            || self
                .changed_contexts
                .iter()
                .any(|context| path.starts_with(context) || context.starts_with(path))
            || path_is_context
                && self
                    .modified_files
                    .iter()
                    .chain(&self.removed_files)
                    .any(|changed| changed.starts_with(path))
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UnsafeWatchCache {
    inner: Arc<Mutex<UnsafeWatchCacheInner>>,
}

#[derive(Debug, Default)]
struct UnsafeWatchCacheInner {
    resolves: FxHashMap<ResolveRequest, Arc<ResolveRecord>>,
    previous_resolves: FxHashMap<ResolveRequest, Arc<ResolveRecord>>,
    module_builds: FxHashMap<ModuleIdentity, UnsafeModuleBuildRecord>,
    previous_module_builds: FxHashMap<ModuleIdentity, UnsafeModuleBuildRecord>,
}

#[derive(Debug)]
struct UnsafeModuleBuildRecord {
    etag: CacheETag,
    record: Arc<ModuleBuildRecord>,
}

pub(crate) enum UnsafeWatchCacheLookup<T> {
    Reusable(T),
    Invalidated,
    Miss,
}

impl UnsafeWatchCache {
    pub(crate) fn begin_compilation(&self, can_reuse_previous: bool) {
        let mut inner = self
            .inner
            .lock()
            .expect("unsafe watch cache mutex should not be poisoned");
        if can_reuse_previous {
            inner.previous_resolves = std::mem::take(&mut inner.resolves);
            inner.previous_module_builds = std::mem::take(&mut inner.module_builds);
        } else {
            inner.resolves.clear();
            inner.previous_resolves.clear();
            inner.module_builds.clear();
            inner.previous_module_builds.clear();
        }
    }

    pub(crate) fn get_resolve(
        &self,
        request: &ResolveRequest,
        changes: &WatchChangeSet,
    ) -> UnsafeWatchCacheLookup<Arc<ResolveRecord>> {
        let mut inner = self
            .inner
            .lock()
            .expect("unsafe watch cache mutex should not be poisoned");
        let Some(record) = inner.previous_resolves.get(request).cloned() else {
            return UnsafeWatchCacheLookup::Miss;
        };
        if record.snapshot().is_affected_by(changes) {
            inner.previous_resolves.remove(request);
            UnsafeWatchCacheLookup::Invalidated
        } else {
            inner.resolves.insert(request.clone(), Arc::clone(&record));
            UnsafeWatchCacheLookup::Reusable(record)
        }
    }

    pub(crate) fn remember_resolve(&self, request: ResolveRequest, record: Arc<ResolveRecord>) {
        self.inner
            .lock()
            .expect("unsafe watch cache mutex should not be poisoned")
            .resolves
            .insert(request, record);
    }

    pub(crate) fn get_module_build(
        &self,
        identity: &ModuleIdentity,
        etag: &CacheETag,
        changes: &WatchChangeSet,
    ) -> UnsafeWatchCacheLookup<Arc<ModuleBuildRecord>> {
        let mut inner = self
            .inner
            .lock()
            .expect("unsafe watch cache mutex should not be poisoned");
        let affected = inner
            .previous_module_builds
            .get(identity)
            .is_some_and(|cached| cached.record.snapshot().is_affected_by(changes));
        if affected {
            inner.previous_module_builds.remove(identity);
            return UnsafeWatchCacheLookup::Invalidated;
        }
        let Some(cached) = inner.previous_module_builds.get(identity) else {
            return UnsafeWatchCacheLookup::Miss;
        };
        if cached.etag == *etag {
            let record = Arc::clone(&cached.record);
            inner.module_builds.insert(
                identity.clone(),
                UnsafeModuleBuildRecord {
                    etag: etag.clone(),
                    record: Arc::clone(&record),
                },
            );
            UnsafeWatchCacheLookup::Reusable(record)
        } else {
            UnsafeWatchCacheLookup::Miss
        }
    }

    pub(crate) fn remember_module_build(
        &self,
        identity: ModuleIdentity,
        etag: CacheETag,
        record: Arc<ModuleBuildRecord>,
    ) {
        self.inner
            .lock()
            .expect("unsafe watch cache mutex should not be poisoned")
            .module_builds
            .insert(identity, UnsafeModuleBuildRecord { etag, record });
    }
}
