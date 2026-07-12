// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/Cache.js

//! Webpack-aligned Cache coordinator for ordered Cache Layer lookup, promotion, storage, and work accounting.

mod build_cache;
mod cache_items;
mod memory_cache_plugin;
mod memory_with_gc_cache_plugin;
mod options;
pub(crate) mod pack_file;
mod pack_file_cache_strategy;

pub use options::{BuildDependency, CacheCompression, CacheKind, CacheOptions};

pub(crate) use build_cache::BuildCache;
pub(crate) use cache_items::{ModuleBuildRecord, ResolveRecord, ResolveRequest};

use std::{any::Any, fmt, io, sync::Arc, time::Duration};

use self::pack_file::{AccessStamp, PackFileGuardDto};

use self::{
    build_cache::CacheDiagnostics,
    memory_cache_plugin::MemoryCacheLayer,
    memory_with_gc_cache_plugin::MemoryWithGcCacheLayer,
    pack_file_cache_strategy::{PackFileCacheLayer, PersistentCachePreparation, PersistentRestore},
};
use crate::cache_facade::{CacheAddress, CacheETag};
#[cfg(test)]
use build_cache::RestoreBarrier;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CacheItemFamily {
    Resolve,
    ModuleBuild,
    CodeGeneration,
    AssetRender,
}

impl CacheItemFamily {
    const fn index(self) -> usize {
        match self {
            Self::Resolve => 0,
            Self::ModuleBuild => 1,
            Self::CodeGeneration => 2,
            Self::AssetRender => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CacheItemWork {
    pub hits: usize,
    pub misses: usize,
    pub stores: usize,
    pub restores: usize,
    pub evictions: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CacheWorkCounters {
    by_family: [CacheItemWork; 4],
}

impl CacheWorkCounters {
    pub(crate) fn for_family(self, family: CacheItemFamily) -> CacheItemWork {
        self.by_family[family.index()]
    }

    fn for_family_mut(&mut self, family: CacheItemFamily) -> &mut CacheItemWork {
        &mut self.by_family[family.index()]
    }
}

#[derive(Clone)]
pub(super) struct CacheEntry {
    pub(super) family: CacheItemFamily,
    pub(super) etag: Option<CacheETag>,
    value: Arc<dyn Any + Send + Sync>,
}

impl CacheEntry {
    pub(super) fn new<V>(family: CacheItemFamily, etag: Option<CacheETag>, value: V) -> Self
    where
        V: Send + Sync + 'static,
    {
        Self {
            family,
            etag,
            value: Arc::new(value),
        }
    }

    pub(super) fn value<V: Send + Sync + 'static>(&self) -> Option<Arc<V>> {
        Arc::clone(&self.value).downcast::<V>().ok()
    }
}

impl fmt::Debug for CacheEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CacheEntry")
            .field("family", &self.family)
            .field("etag", &self.etag)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(super) enum CacheLayerLookup {
    Hit(CacheEntry),
    Deferred(PersistentRestore),
    Miss,
}

pub(super) trait CacheLayer: fmt::Debug + Send + Sync {
    fn lookup(&mut self, address: &CacheAddress, etag: Option<&CacheETag>) -> CacheLayerLookup;
    fn store(&mut self, address: CacheAddress, entry: CacheEntry);
    fn clear(&mut self) {}
}

#[derive(Debug)]
enum CacheLayerSlot {
    Memory(MemoryCacheLayer),
    MemoryWithGc(MemoryWithGcCacheLayer),
    Persistent {
        writable: bool,
        layer: PackFileCacheLayer,
    },
}

impl CacheLayerSlot {
    fn writable(&self) -> bool {
        match self {
            Self::Memory(_) | Self::MemoryWithGc(_) => true,
            Self::Persistent { writable, .. } => *writable,
        }
    }

    fn layer_mut(&mut self) -> &mut dyn CacheLayer {
        match self {
            Self::Memory(layer) => layer,
            Self::MemoryWithGc(layer) => layer,
            Self::Persistent { layer, .. } => layer,
        }
    }

    #[cfg(test)]
    fn memory_entry_count(&self, family: CacheItemFamily) -> Option<usize> {
        match self {
            Self::Memory(layer) => Some(layer.entry_count(family)),
            Self::MemoryWithGc(layer) => Some(layer.entry_count(family)),
            Self::Persistent { .. } => None,
        }
    }

    #[cfg(test)]
    fn evict_memory(&mut self, address: &CacheAddress) -> Option<bool> {
        match self {
            Self::Memory(layer) => Some(layer.evict(address)),
            Self::MemoryWithGc(layer) => Some(layer.evict(address)),
            Self::Persistent { .. } => None,
        }
    }

    fn persistent(&self) -> Option<&PackFileCacheLayer> {
        match self {
            Self::Memory(_) | Self::MemoryWithGc(_) => None,
            Self::Persistent { layer, .. } => Some(layer),
        }
    }

    fn persistent_mut(&mut self) -> Option<&mut PackFileCacheLayer> {
        match self {
            Self::Memory(_) | Self::MemoryWithGc(_) => None,
            Self::Persistent { layer, .. } => Some(layer),
        }
    }
}

#[derive(Debug)]
pub(super) struct Cache {
    layers: Vec<CacheLayerSlot>,
    work: CacheWorkCounters,
}

pub(super) struct CacheRestore {
    pub(super) layer_index: usize,
    pub(super) restore: PersistentRestore,
}

pub(super) enum CacheGet<V> {
    Ready {
        value: Option<Arc<V>>,
        persistent_access_changed: bool,
    },
    Deferred(CacheRestore),
}

impl<V> CacheGet<V> {
    pub(super) fn persistent_access_changed(&self) -> bool {
        matches!(
            self,
            Self::Ready {
                persistent_access_changed: true,
                ..
            }
        )
    }
}

impl Cache {
    pub(super) fn from_options(options: &CacheOptions, diagnostics: Arc<CacheDiagnostics>) -> Self {
        let mut layers = Vec::new();
        if options.kind != CacheKind::Disabled {
            match options.max_memory_generations {
                Some(0) => {}
                Some(generations) => layers.push(CacheLayerSlot::MemoryWithGc(
                    MemoryWithGcCacheLayer::new(generations),
                )),
                None => layers.push(CacheLayerSlot::Memory(MemoryCacheLayer::new())),
            }
        }
        if options.kind == CacheKind::Filesystem {
            layers.push(CacheLayerSlot::Persistent {
                writable: !options.readonly,
                layer: PackFileCacheLayer::open(options, diagnostics),
            });
        }
        Self {
            layers,
            work: CacheWorkCounters::default(),
        }
    }

    pub(super) fn begin_get<V>(
        &mut self,
        family: CacheItemFamily,
        address: &CacheAddress,
        etag: Option<&CacheETag>,
        stamp: AccessStamp,
    ) -> CacheGet<V>
    where
        V: Send + Sync + 'static,
    {
        for index in 0..self.layers.len() {
            match self.layers[index].layer_mut().lookup(address, etag) {
                CacheLayerLookup::Hit(entry) => {
                    if let Some((value, persistent_access_changed)) =
                        self.use_entry(family, address, etag, stamp, index, entry)
                    {
                        return CacheGet::Ready {
                            value: Some(value),
                            persistent_access_changed,
                        };
                    }
                }
                CacheLayerLookup::Deferred(restore) => {
                    return CacheGet::Deferred(CacheRestore {
                        layer_index: index,
                        restore,
                    });
                }
                CacheLayerLookup::Miss => {}
            }
        }

        self.work.for_family_mut(family).misses += 1;
        CacheGet::Ready {
            value: None,
            persistent_access_changed: false,
        }
    }

    pub(super) fn finish_restore<V>(
        &mut self,
        family: CacheItemFamily,
        address: &CacheAddress,
        etag: Option<&CacheETag>,
        stamp: AccessStamp,
        plan: CacheRestore,
        restored: Option<CacheEntry>,
    ) -> CacheGet<V>
    where
        V: Send + Sync + 'static,
    {
        for index in 0..plan.layer_index {
            if let CacheLayerLookup::Hit(entry) =
                self.layers[index].layer_mut().lookup(address, etag)
                && let Some((value, persistent_access_changed)) =
                    self.use_entry(family, address, etag, stamp, index, entry)
            {
                return CacheGet::Ready {
                    value: Some(value),
                    persistent_access_changed,
                };
            }
        }

        match self.layers[plan.layer_index]
            .layer_mut()
            .lookup(address, etag)
        {
            CacheLayerLookup::Hit(entry) => {
                if let Some((value, persistent_access_changed)) =
                    self.use_entry(family, address, etag, stamp, plan.layer_index, entry)
                {
                    return CacheGet::Ready {
                        value: Some(value),
                        persistent_access_changed,
                    };
                }
            }
            CacheLayerLookup::Deferred(current) if current.reads_from(&plan.restore) => {
                if let Some(entry) = restored
                    && let Some((value, persistent_access_changed)) =
                        self.use_entry(family, address, etag, stamp, plan.layer_index, entry)
                {
                    return CacheGet::Ready {
                        value: Some(value),
                        persistent_access_changed,
                    };
                }
            }
            CacheLayerLookup::Deferred(current) => {
                return CacheGet::Deferred(CacheRestore {
                    layer_index: plan.layer_index,
                    restore: current,
                });
            }
            CacheLayerLookup::Miss => {}
        }

        self.work.for_family_mut(family).misses += 1;
        CacheGet::Ready {
            value: None,
            persistent_access_changed: false,
        }
    }

    fn use_entry<V>(
        &mut self,
        family: CacheItemFamily,
        address: &CacheAddress,
        etag: Option<&CacheETag>,
        stamp: AccessStamp,
        layer_index: usize,
        entry: CacheEntry,
    ) -> Option<(Arc<V>, bool)>
    where
        V: Send + Sync + 'static,
    {
        let value = entry.value::<V>()?;
        if layer_index > 0 {
            for earlier in &mut self.layers[..layer_index] {
                if earlier.writable() {
                    earlier.layer_mut().store(address.clone(), entry.clone());
                }
            }
            self.work.for_family_mut(family).restores += 1;
        }
        self.work.for_family_mut(family).hits += 1;
        let persistent_access_changed = self
            .layers
            .iter()
            .filter(|slot| slot.writable())
            .find_map(CacheLayerSlot::persistent)
            .and_then(|layer| layer.plan_touch(address, etag, stamp))
            .is_some_and(|touch| touch.apply());
        Some((value, persistent_access_changed))
    }

    pub(super) fn store<V>(
        &mut self,
        family: CacheItemFamily,
        address: CacheAddress,
        etag: Option<CacheETag>,
        value: V,
    ) -> bool
    where
        V: Send + Sync + 'static,
    {
        let entry = CacheEntry::new(family, etag, value);
        let mut stored_persistently = false;
        for slot in &mut self.layers {
            if !slot.writable() {
                continue;
            }
            stored_persistently |= slot.persistent().is_some();
            slot.layer_mut().store(address.clone(), entry.clone());
        }
        self.work.for_family_mut(family).stores += 1;
        stored_persistently
    }

    #[cfg(test)]
    pub(super) fn evict_memory(&mut self, family: CacheItemFamily, address: &CacheAddress) {
        let evicted = self
            .layers
            .iter_mut()
            .filter_map(|layer| layer.evict_memory(address))
            .any(|evicted| evicted);
        if evicted {
            self.work.for_family_mut(family).evictions += 1;
        }
    }

    #[cfg(test)]
    pub(super) fn entry_count(&self, family: CacheItemFamily) -> usize {
        self.layers
            .iter()
            .find_map(|layer| layer.memory_entry_count(family))
            .unwrap_or_default()
    }

    pub(super) fn work_counters(&self) -> CacheWorkCounters {
        self.work
    }

    pub(super) fn clear(&mut self) {
        for slot in &mut self.layers {
            slot.layer_mut().clear();
        }
    }

    pub(super) fn prepare_persistent(
        &mut self,
        preparation: PersistentCachePreparation<'_>,
    ) -> bool {
        self.layers
            .iter_mut()
            .find_map(CacheLayerSlot::persistent_mut)
            .is_some_and(|layer| layer.prepare_persistent(preparation))
    }

    pub(super) fn on_compilation_completed(&mut self) {
        for slot in &mut self.layers {
            let CacheLayerSlot::MemoryWithGc(layer) = slot else {
                continue;
            };
            for family in layer.on_compilation_completed() {
                self.work.for_family_mut(family).evictions += 1;
            }
        }
    }

    pub(super) fn publish_persistent(
        &mut self,
        guard: PackFileGuardDto,
        stamp: AccessStamp,
        max_age: Duration,
    ) -> io::Result<()> {
        let Some(layer) = self
            .layers
            .iter_mut()
            .find_map(CacheLayerSlot::persistent_mut)
        else {
            return Ok(());
        };
        layer.publish(guard, stamp, max_age)
    }

    pub(super) fn has_persistent_publication(&self) -> bool {
        self.layers
            .iter()
            .find_map(CacheLayerSlot::persistent)
            .is_some_and(PackFileCacheLayer::has_publication)
    }

    #[cfg(test)]
    pub(super) fn install_restore_barrier(&mut self, barrier: RestoreBarrier) {
        if let Some(layer) = self
            .layers
            .iter_mut()
            .find_map(CacheLayerSlot::persistent_mut)
        {
            layer.install_restore_barrier(barrier);
        }
    }
}
