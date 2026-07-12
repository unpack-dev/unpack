// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/CacheFacade.js

//! Webpack-aligned Cache Facade and cache-item identity primitives.
//! This is the narrow caller-facing seam over the shared Cache coordinator.

use std::{marker::PhantomData, sync::Arc};

use crate::{
    ModuleIdentity,
    cache::pack_file::{PackFileAddress, PackFileETag},
    cache::{Cache, CacheItemFamily, ModuleBuildRecord, ResolveRecord, ResolveRequest},
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
    pub(super) cache: Cache,
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

    pub(crate) fn from_borrowed_parts<'a, I>(parts: I) -> Self
    where
        I: IntoIterator<Item = &'a [u8]>,
        I::IntoIter: Clone,
    {
        let parts = parts.into_iter();
        let capacity = parts
            .clone()
            .map(|part| size_of::<u64>() + part.len())
            .sum();
        let mut bytes = Vec::with_capacity(capacity);
        for part in parts {
            bytes.extend_from_slice(&(part.len() as u64).to_le_bytes());
            bytes.extend_from_slice(part);
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
        self.cache.is_enabled()
    }

    #[allow(dead_code)]
    pub(crate) fn namespace(&self) -> CacheNamespace {
        self.namespace
    }

    pub(crate) fn get(&self, key: &K, etag: Option<&CacheETag>) -> Option<Arc<V>> {
        let address = CacheAddress {
            namespace: self.namespace,
            identifier: key.cache_identifier(),
        };
        self.cache.get(self.family, &address, etag)
    }

    pub(crate) fn store(&self, key: K, etag: Option<CacheETag>, value: V) {
        let address = CacheAddress {
            namespace: self.namespace,
            identifier: key.cache_identifier(),
        };
        self.cache.store(self.family, address, etag, value);
    }

    #[cfg(test)]
    pub(crate) fn evict_memory(&self, key: &K) {
        let address = CacheAddress {
            namespace: self.namespace,
            identifier: key.cache_identifier(),
        };
        self.cache.evict_memory(self.family, &address);
    }
}
