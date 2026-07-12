// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/cache/MemoryWithGcCachePlugin.js

//! Webpack-aligned Memory Cache Plugin with generation-based garbage collection.

use std::collections::HashMap;

use super::{CacheEntry, CacheItemFamily, CacheLayer, CacheLayerLookup};
use crate::cache_facade::{CacheAddress, CacheETag};

#[derive(Debug, Clone)]
struct MemoryCacheEntry {
    entry: CacheEntry,
    last_used_generation: u64,
}

#[derive(Debug)]
pub(super) struct MemoryWithGcCacheLayer {
    entries: HashMap<CacheAddress, MemoryCacheEntry>,
    max_unused_generations: u64,
    completed_generation: u64,
}

impl MemoryWithGcCacheLayer {
    pub(super) fn new(max_unused_generations: u64) -> Self {
        Self {
            entries: HashMap::new(),
            max_unused_generations,
            completed_generation: 0,
        }
    }

    fn active_generation(&self) -> u64 {
        self.completed_generation.saturating_add(1)
    }

    pub(super) fn on_compilation_completed(&mut self) -> Vec<CacheItemFamily> {
        self.completed_generation = self.completed_generation.saturating_add(1);
        let completed_generation = self.completed_generation;
        let mut evicted = Vec::new();
        self.entries.retain(|_, entry| {
            let should_retain = completed_generation.saturating_sub(entry.last_used_generation)
                < self.max_unused_generations;
            if !should_retain {
                evicted.push(entry.entry.family);
            }
            should_retain
        });
        evicted
    }

    #[cfg(test)]
    pub(super) fn evict(&mut self, address: &CacheAddress) -> bool {
        self.entries.remove(address).is_some()
    }

    #[cfg(test)]
    pub(super) fn entry_count(&self, family: CacheItemFamily) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.entry.family == family)
            .count()
    }
}

impl CacheLayer for MemoryWithGcCacheLayer {
    fn lookup(&mut self, address: &CacheAddress, etag: Option<&CacheETag>) -> CacheLayerLookup {
        let active_generation = self.active_generation();
        let Some(entry) = self
            .entries
            .get_mut(address)
            .filter(|entry| entry.entry.etag.as_ref() == etag)
        else {
            return CacheLayerLookup::Miss;
        };
        entry.last_used_generation = active_generation;
        CacheLayerLookup::Hit(entry.entry.clone())
    }

    fn store(&mut self, address: CacheAddress, entry: CacheEntry) {
        self.entries.insert(
            address,
            MemoryCacheEntry {
                entry,
                last_used_generation: self.active_generation(),
            },
        );
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}
