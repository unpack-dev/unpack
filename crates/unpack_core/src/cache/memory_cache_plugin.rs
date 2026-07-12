//! Webpack-aligned in-process Memory Cache Plugin.

use std::collections::HashMap;

#[cfg(test)]
use super::CacheItemFamily;
use super::{CacheEntry, CacheLayer, CacheLayerLookup};
use crate::cache_facade::{CacheAddress, CacheETag};

#[derive(Debug, Default)]
pub(super) struct MemoryCacheLayer {
    entries: HashMap<CacheAddress, CacheEntry>,
}

impl MemoryCacheLayer {
    pub(super) fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(super) fn evict(&mut self, address: &CacheAddress) -> bool {
        self.entries.remove(address).is_some()
    }

    #[cfg(test)]
    pub(super) fn entry_count(&self, family: CacheItemFamily) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.family == family)
            .count()
    }
}

impl CacheLayer for MemoryCacheLayer {
    fn lookup(&mut self, address: &CacheAddress, etag: Option<&CacheETag>) -> CacheLayerLookup {
        self.entries
            .get(address)
            .filter(|entry| entry.etag.as_ref() == etag)
            .map_or(CacheLayerLookup::Miss, |entry| {
                CacheLayerLookup::Hit(entry.clone())
            })
    }

    fn store(&mut self, address: CacheAddress, entry: CacheEntry) {
        self.entries.insert(address, entry);
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}
