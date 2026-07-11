//! In-process Memory Cache Layer with optional generation-based retention.

use std::collections::HashMap;

use super::{
    cache::{CacheEntry, CacheItemFamily, CacheLayer, CacheLayerLookup},
    facade::{CacheAddress, CacheETag},
    options::{CacheKind, CacheOptions},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MemoryRetention {
    Disabled,
    Generations(u64),
    Unbounded,
}

impl MemoryRetention {
    pub(super) fn from_options(options: &CacheOptions) -> Self {
        match options.kind {
            CacheKind::Disabled => Self::Disabled,
            CacheKind::Memory | CacheKind::Filesystem => match options.max_memory_generations {
                Some(0) => Self::Disabled,
                Some(generations) => Self::Generations(generations),
                None => Self::Unbounded,
            },
        }
    }
}

#[derive(Debug, Clone)]
struct MemoryCacheEntry {
    entry: CacheEntry,
    last_used_generation: u64,
}

#[derive(Debug)]
pub(super) struct MemoryCacheLayer {
    entries: HashMap<CacheAddress, MemoryCacheEntry>,
    retention: MemoryRetention,
    completed_generation: u64,
}

impl MemoryCacheLayer {
    pub(super) fn new(retention: MemoryRetention) -> Self {
        Self {
            entries: HashMap::new(),
            retention,
            completed_generation: 0,
        }
    }

    fn active_generation(&self) -> u64 {
        self.completed_generation.saturating_add(1)
    }

    pub(super) fn on_compilation_completed(&mut self) -> Vec<CacheItemFamily> {
        let MemoryRetention::Generations(limit) = self.retention else {
            return Vec::new();
        };
        self.completed_generation = self.completed_generation.saturating_add(1);
        let completed_generation = self.completed_generation;
        let mut evicted = Vec::new();
        self.entries.retain(|_, entry| {
            let should_retain =
                completed_generation.saturating_sub(entry.last_used_generation) < limit;
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

impl CacheLayer for MemoryCacheLayer {
    fn lookup(&mut self, address: &CacheAddress, etag: Option<&CacheETag>) -> CacheLayerLookup {
        if self.retention == MemoryRetention::Unbounded {
            return self
                .entries
                .get(address)
                .filter(|entry| entry.entry.etag.as_ref() == etag)
                .map_or(CacheLayerLookup::Miss, |entry| {
                    CacheLayerLookup::Hit(entry.entry.clone())
                });
        }
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
