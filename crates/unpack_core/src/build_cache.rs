use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};

use crate::{ModuleIdentity, SnapshotStrategy, parser::ParsedModule, snapshot::FileSnapshot};

#[derive(Debug, Clone, Default)]
pub(crate) struct BuildCache {
    inner: Arc<Mutex<BuildCacheInner>>,
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
    pub(crate) fn get_module_build(&self, identity: &ModuleIdentity) -> Option<ModuleBuildRecord> {
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
