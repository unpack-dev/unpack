use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{ModuleIdentity, SnapshotStrategy, parser::ParsedModule, snapshot::FileSnapshot};
use serde::{Deserialize, Serialize};

const CACHE_MAGIC: &str = "UNPACK_PERSISTENT_CACHE";
const PACK_MAGIC: &[u8] = b"UNPACK-CACHE-PACK\0";
const CACHE_SCHEMA_VERSION: u32 = 1;
const DEFAULT_PACK_FILE: &str = "packs/modules.cbor";
const MANIFEST_FILE: &str = "container.json";

#[derive(Debug, Clone)]
pub(crate) struct BuildCache {
    options: CacheOptions,
    build_dependency_snapshot_strategy: SnapshotStrategy,
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
    dirty: bool,
    module_hits: usize,
    module_misses: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub(crate) fn new(
        options: CacheOptions,
        build_dependency_snapshot_strategy: SnapshotStrategy,
    ) -> Self {
        let cache = Self {
            options,
            build_dependency_snapshot_strategy,
            inner: Arc::new(Mutex::new(BuildCacheInner::default())),
        };
        cache.restore_from_filesystem();
        cache
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

        {
            let mut inner = self
                .inner
                .lock()
                .expect("build cache mutex should not be poisoned");
            inner.module_builds.insert(identity, record);
            if self.options.kind == CacheKind::Filesystem {
                inner.dirty = true;
            }
        }
    }

    pub(crate) fn flush_to_filesystem(&self) -> io::Result<()> {
        if self.options.kind != CacheKind::Filesystem {
            return Ok(());
        }

        let module_builds = {
            let inner = self
                .inner
                .lock()
                .expect("build cache mutex should not be poisoned");
            if !inner.dirty {
                return Ok(());
            }
            inner.module_builds.clone()
        };

        self.write_filesystem_cache(&module_builds)?;

        self.inner
            .lock()
            .expect("build cache mutex should not be poisoned")
            .dirty = false;
        Ok(())
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

impl BuildCache {
    fn restore_from_filesystem(&self) {
        if self.options.kind != CacheKind::Filesystem {
            return;
        }
        let Some(cache_location) = &self.options.cache_location else {
            return;
        };
        let Some(manifest) = read_manifest(cache_location) else {
            return;
        };
        if !self.manifest_is_valid(&manifest) {
            return;
        }

        let Some(pack) = read_pack(cache_location, &manifest.pack_file) else {
            return;
        };
        if pack.magic != CACHE_MAGIC
            || pack.schema_version != CACHE_SCHEMA_VERSION
            || pack.cache_version != self.cache_version()
        {
            return;
        }

        self.inner
            .lock()
            .expect("build cache mutex should not be poisoned")
            .module_builds = pack.module_builds.into_iter().collect();
    }

    fn write_filesystem_cache(
        &self,
        module_builds: &HashMap<ModuleIdentity, ModuleBuildRecord>,
    ) -> io::Result<()> {
        let Some(cache_location) = &self.options.cache_location else {
            return Ok(());
        };

        let pack_file = PathBuf::from(DEFAULT_PACK_FILE);
        let pack_path = cache_location.join(&pack_file);
        if let Some(parent) = pack_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let cache_version = self.cache_version();
        let pack = CachePackDto {
            magic: CACHE_MAGIC.to_string(),
            schema_version: CACHE_SCHEMA_VERSION,
            cache_version: cache_version.clone(),
            module_builds: module_builds
                .iter()
                .map(|(identity, record)| (identity.clone(), record.clone()))
                .collect(),
        };
        let pack_payload = cbor4ii::serde::to_vec(Vec::new(), &pack)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut pack_bytes = PACK_MAGIC.to_vec();
        pack_bytes.extend(pack_payload);
        fs::write(pack_path, pack_bytes)?;

        let manifest = CacheManifest {
            magic: CACHE_MAGIC.to_string(),
            schema_version: CACHE_SCHEMA_VERSION,
            cache_version,
            pack_file: pack_file.to_string_lossy().replace('\\', "/"),
            build_dependencies: self.build_dependency_snapshots()?,
        };
        fs::create_dir_all(cache_location)?;
        let manifest_json = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(cache_location.join(MANIFEST_FILE), manifest_json)?;
        Ok(())
    }

    fn manifest_is_valid(&self, manifest: &CacheManifest) -> bool {
        manifest.magic == CACHE_MAGIC
            && manifest.schema_version == CACHE_SCHEMA_VERSION
            && manifest.cache_version == self.cache_version()
            && self.build_dependency_snapshots_are_valid(&manifest.build_dependencies)
    }

    fn cache_version(&self) -> String {
        self.options.version.clone().unwrap_or_default()
    }

    fn build_dependency_snapshots(&self) -> io::Result<Vec<PersistentBuildDependencySnapshot>> {
        self.options
            .build_dependencies
            .iter()
            .map(|dependency| {
                Ok(PersistentBuildDependencySnapshot {
                    name: dependency.name.clone(),
                    files: dependency
                        .files
                        .iter()
                        .map(|path| {
                            Ok(PersistentFileSnapshot {
                                path: path.clone(),
                                snapshot: FileSnapshot::create_from_file_sync(
                                    path,
                                    self.build_dependency_snapshot_strategy,
                                )
                                .map_err(|error| {
                                    io::Error::new(io::ErrorKind::InvalidData, error)
                                })?,
                            })
                        })
                        .collect::<io::Result<Vec<_>>>()?,
                })
            })
            .collect()
    }

    fn build_dependency_snapshots_are_valid(
        &self,
        snapshots: &[PersistentBuildDependencySnapshot],
    ) -> bool {
        if snapshots.len() != self.options.build_dependencies.len() {
            return false;
        }

        self.options.build_dependencies.iter().all(|dependency| {
            let Some(snapshot) = snapshots
                .iter()
                .find(|snapshot| snapshot.name == dependency.name)
            else {
                return false;
            };
            if snapshot.files.len() != dependency.files.len() {
                return false;
            }

            dependency.files.iter().all(|path| {
                snapshot.files.iter().any(|snapshot_file| {
                    snapshot_file.path == *path
                        && snapshot_file
                            .snapshot
                            .is_valid_sync(path, self.build_dependency_snapshot_strategy)
                })
            })
        })
    }
}

fn read_manifest(cache_location: &Path) -> Option<CacheManifest> {
    let bytes = fs::read(cache_location.join(MANIFEST_FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn read_pack(cache_location: &Path, pack_file: &str) -> Option<CachePackDto> {
    let bytes = fs::read(cache_location.join(pack_file)).ok()?;
    let payload = bytes.strip_prefix(PACK_MAGIC)?;
    cbor4ii::serde::from_slice(payload).ok()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheManifest {
    magic: String,
    schema_version: u32,
    cache_version: String,
    pack_file: String,
    build_dependencies: Vec<PersistentBuildDependencySnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistentBuildDependencySnapshot {
    name: String,
    files: Vec<PersistentFileSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistentFileSnapshot {
    path: PathBuf,
    snapshot: FileSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachePackDto {
    magic: String,
    schema_version: u32,
    cache_version: String,
    module_builds: Vec<(ModuleIdentity, ModuleBuildRecord)>,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuildCacheStats {
    pub module_entries: usize,
    pub module_hits: usize,
    pub module_misses: usize,
}
