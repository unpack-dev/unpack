use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{ModuleIdentity, SnapshotStrategy, parser::ParsedModule, snapshot::FileSnapshot};

#[derive(Debug, Clone, Default)]
pub(crate) struct BuildCache {
    options: CacheOptions,
    inner: Arc<Mutex<BuildCacheInner>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheOptions {
    pub kind: CacheKind,
    pub cache_directory: Option<PathBuf>,
    pub cache_location: Option<PathBuf>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub build_dependencies: Vec<BuildDependency>,
    pub max_memory_generations: Option<u32>,
    pub idle_timeout: Option<u32>,
}

impl Default for CacheOptions {
    fn default() -> Self {
        Self::memory()
    }
}

impl CacheOptions {
    pub fn disabled() -> Self {
        Self {
            kind: CacheKind::Disabled,
            cache_directory: None,
            cache_location: None,
            name: None,
            version: None,
            build_dependencies: Vec::new(),
            max_memory_generations: None,
            idle_timeout: None,
        }
    }

    pub fn memory() -> Self {
        Self {
            kind: CacheKind::Memory,
            cache_directory: None,
            cache_location: None,
            name: None,
            version: None,
            build_dependencies: Vec::new(),
            max_memory_generations: None,
            idle_timeout: None,
        }
    }

    pub fn filesystem() -> Self {
        Self {
            kind: CacheKind::Filesystem,
            cache_directory: None,
            cache_location: None,
            name: None,
            version: None,
            build_dependencies: Vec::new(),
            max_memory_generations: None,
            idle_timeout: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheKind {
    Disabled,
    Memory,
    Filesystem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildDependency {
    pub name: String,
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Default)]
struct BuildCacheInner {
    module_builds: HashMap<ModuleIdentity, ModuleBuildRecord>,
    module_hits: usize,
    module_misses: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModuleBuildRecord {
    parsed: ParsedModule,
    source: String,
    snapshot: FileSnapshot,
}

impl ModuleBuildRecord {
    pub(crate) fn new(parsed: ParsedModule, source: String, snapshot: FileSnapshot) -> Self {
        Self {
            parsed,
            source,
            snapshot,
        }
    }

    pub(crate) fn parsed(&self) -> &ParsedModule {
        &self.parsed
    }

    pub(crate) fn into_parts(self) -> (ParsedModule, String) {
        (self.parsed, self.source)
    }

    pub(crate) async fn is_valid(&self, path: &Path, strategy: SnapshotStrategy) -> bool {
        self.snapshot.is_valid(path, strategy).await
    }
}

impl BuildCache {
    pub(crate) fn new(options: CacheOptions) -> Self {
        Self {
            options,
            inner: Arc::new(Mutex::new(BuildCacheInner::default())),
        }
    }

    pub(crate) fn get_module_build(&self, identity: &ModuleIdentity) -> Option<ModuleBuildRecord> {
        if self.options.kind == CacheKind::Disabled {
            return None;
        }

        let mut inner = self
            .inner
            .lock()
            .expect("build cache mutex should not be poisoned");
        let record = inner.module_builds.get(identity).cloned();
        if record.is_some() {
            inner.module_hits += 1;
        } else {
            inner.module_misses += 1;
        }
        record
    }

    pub(crate) fn store_module_build(&self, identity: ModuleIdentity, record: ModuleBuildRecord) {
        if self.options.kind == CacheKind::Disabled {
            return;
        }

        self.inner
            .lock()
            .expect("build cache mutex should not be poisoned")
            .module_builds
            .insert(identity, record);
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> BuildCacheStats {
        let inner = self
            .inner
            .lock()
            .expect("build cache mutex should not be poisoned");
        BuildCacheStats {
            module_entries: inner.module_builds.len(),
            module_hits: inner.module_hits,
            module_misses: inner.module_misses,
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuildCacheStats {
    pub module_entries: usize,
    pub module_hits: usize,
    pub module_misses: usize,
}
