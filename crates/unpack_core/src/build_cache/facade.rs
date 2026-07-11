//! Typed Cache Facade and cache-item identity primitives.
//! This is the narrow caller-facing seam over the shared Cache coordinator.

use std::{marker::PhantomData, sync::Arc};

use crate::{
    ModuleIdentity,
    pack_file::{AccessStamp, PackFileAddress, PackFileETag},
};

use super::{
    BuildCache,
    cache::{CacheGet, CacheItemFamily},
    options::CacheKind,
    records::{ModuleBuildRecord, ResolveRecord, ResolveRequest},
};

pub(super) const RESOLVE_CACHE_NAMESPACE: CacheNamespace = CacheNamespace::new("unpack/resolve");
pub(super) const MODULE_BUILD_CACHE_NAMESPACE: CacheNamespace =
    CacheNamespace::new("unpack/module-build");
pub(super) const CODE_GENERATION_CACHE_NAMESPACE: CacheNamespace =
    CacheNamespace::new("unpack/code-generation");
pub(super) const ASSET_RENDER_CACHE_NAMESPACE: CacheNamespace =
    CacheNamespace::new("unpack/asset-render");

#[derive(Debug, Clone)]
pub(crate) struct CacheFacade<K, V> {
    pub(super) build_cache: BuildCache,
    pub(super) namespace: CacheNamespace,
    pub(super) family: CacheItemFamily,
    pub(super) marker: PhantomData<fn(K) -> V>,
}

pub(crate) type NormalModuleFactoryCache = CacheFacade<ResolveRequest, ResolveRecord>;
pub(crate) type ModuleBuildCache = CacheFacade<ModuleIdentity, ModuleBuildRecord>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CacheNamespace(&'static str);

impl CacheNamespace {
    pub(crate) const fn new(value: &'static str) -> Self {
        Self(value)
    }

    pub(super) const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CacheIdentifier(Vec<u8>);

impl CacheIdentifier {
    #[allow(dead_code)]
    pub(crate) fn new(value: impl AsRef<[u8]>) -> Self {
        Self(value.as_ref().to_vec())
    }

    pub(crate) fn from_parts(parts: impl IntoIterator<Item = Vec<u8>>) -> Self {
        let mut bytes = Vec::new();
        for part in parts {
            bytes.extend_from_slice(&(part.len() as u64).to_le_bytes());
            bytes.extend_from_slice(&part);
        }
        Self(bytes)
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CacheETag(Vec<u8>);

impl CacheETag {
    #[allow(dead_code)]
    pub(crate) fn new(value: impl AsRef<[u8]>) -> Self {
        Self(value.as_ref().to_vec())
    }

    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

pub(crate) trait CacheKey: Clone + Send + Sync + 'static {
    fn cache_identifier(&self) -> CacheIdentifier;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct CacheAddress {
    pub(super) namespace: CacheNamespace,
    pub(super) identifier: CacheIdentifier,
}

impl CacheAddress {
    pub(super) fn to_pack_file_address(&self) -> PackFileAddress {
        PackFileAddress::new(self.namespace.as_str(), self.identifier.as_bytes())
    }
}

impl CacheETag {
    pub(super) fn to_pack_file_etag(&self) -> PackFileETag {
        PackFileETag::new(self.as_bytes())
    }
}

impl<K, V> CacheFacade<K, V>
where
    K: CacheKey,
    V: Send + Sync + 'static,
{
    pub(crate) fn is_enabled(&self) -> bool {
        self.build_cache.options.kind != CacheKind::Disabled
    }

    #[allow(dead_code)]
    pub(crate) fn namespace(&self) -> CacheNamespace {
        self.namespace
    }

    pub(crate) fn get(&self, key: &K, etag: Option<&CacheETag>) -> Option<Arc<V>> {
        if !self.is_enabled() {
            return None;
        }

        let address = CacheAddress {
            namespace: self.namespace,
            identifier: key.cache_identifier(),
        };
        // Access timestamps only matter for the filesystem layer. Avoid a clock
        // syscall for memory caches and read-only persistent caches, which never
        // record access updates.
        let stamp = if self.build_cache.is_writable_filesystem_cache() {
            self.build_cache.clock.now()
        } else {
            AccessStamp::from_millis(0)
        };

        let mut result = {
            let mut cache = self
                .build_cache
                .inner
                .cache
                .lock()
                .expect("build cache data mutex should not be poisoned");
            let result = cache.begin_get(self.family, &address, etag, stamp);
            if result.persistent_access_changed() {
                self.build_cache.inner.mark_dirty();
            }
            result
        };

        loop {
            match result {
                CacheGet::Ready { value, .. } => return value,
                CacheGet::Deferred(plan) => {
                    let restored = plan.restore.restore();
                    result = {
                        let mut cache = self
                            .build_cache
                            .inner
                            .cache
                            .lock()
                            .expect("build cache data mutex should not be poisoned");
                        let result = cache.finish_restore(
                            self.family,
                            &address,
                            etag,
                            stamp,
                            plan,
                            restored,
                        );
                        if result.persistent_access_changed() {
                            self.build_cache.inner.mark_dirty();
                        }
                        result
                    };
                }
            }
        }
    }

    pub(crate) fn store(&self, key: K, etag: Option<CacheETag>, value: V) {
        if !self.is_enabled() {
            return;
        }

        let address = CacheAddress {
            namespace: self.namespace,
            identifier: key.cache_identifier(),
        };
        let mut cache = self
            .build_cache
            .inner
            .cache
            .lock()
            .expect("build cache data mutex should not be poisoned");
        if cache.store(self.family, address, etag, value) {
            self.build_cache.inner.mark_dirty();
        }
    }

    #[cfg(test)]
    pub(crate) fn evict_memory(&self, key: &K) {
        let address = CacheAddress {
            namespace: self.namespace,
            identifier: key.cache_identifier(),
        };
        self.build_cache
            .inner
            .cache
            .lock()
            .expect("build cache data mutex should not be poisoned")
            .evict_memory(self.family, &address);
    }
}
