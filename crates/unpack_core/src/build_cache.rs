use std::{
    collections::{BTreeSet, HashMap},
    fs,
    hash::Hash,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{
    ModuleIdentity, SnapshotOptions, SnapshotStrategy,
    parser::ParsedModule,
    snapshot::{FileSystemInfo, Snapshot},
};
use serde::{Deserialize, Serialize};

const CACHE_MAGIC: &str = "UNPACK_PERSISTENT_CACHE";
const PACK_MAGIC: &[u8] = b"UNPACK-CACHE-PACK\0";
const DEFAULT_PACK_FILE: &str = "packs/modules.cbor";
const MANIFEST_FILE: &str = "container.json";

#[derive(Debug, Clone)]
pub(crate) struct BuildCache {
    options: CacheOptions,
    build_dependency_snapshot_strategy: SnapshotStrategy,
    resolve_build_dependency_snapshot_strategy: SnapshotStrategy,
    file_system_info: FileSystemInfo,
    inner: Arc<Mutex<BuildCacheInner>>,
}

#[derive(Debug, Clone)]
pub(crate) struct CacheFacade<K, V> {
    build_cache: BuildCache,
    store: CacheStoreAccessor<K, V>,
}

pub(crate) type NormalModuleFactoryCache = CacheFacade<ResolveRequest, ResolveRecord>;
pub(crate) type ModuleBuildCache = CacheFacade<ModuleIdentity, ModuleBuildRecord>;

type CacheStoreAccessor<K, V> = for<'a> fn(&'a mut BuildCacheInner) -> &'a mut CacheStore<K, V>;

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
    resolve_records: CacheStore<ResolveRequest, ResolveRecord>,
    module_builds: CacheStore<ModuleIdentity, ModuleBuildRecord>,
    dirty: bool,
}

#[derive(Debug)]
struct CacheStore<K, V> {
    records: HashMap<K, Arc<V>>,
    hits: usize,
    misses: usize,
}

impl<K, V> Default for CacheStore<K, V> {
    fn default() -> Self {
        Self {
            records: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }
}

impl<K, V> CacheStore<K, V>
where
    K: Eq + Hash,
{
    fn get(&mut self, key: &K) -> Option<Arc<V>> {
        let record = self.records.get(key).cloned();
        if record.is_some() {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        record
    }

    fn store(&mut self, key: K, value: V) {
        self.records.insert(key, Arc::new(value));
    }

    #[cfg(test)]
    fn entries(&self) -> usize {
        self.records.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct ResolveRequest {
    context: PathBuf,
    request: String,
}

impl ResolveRequest {
    pub(crate) fn new(context: impl Into<PathBuf>, request: impl Into<String>) -> Self {
        Self {
            context: context.into(),
            request: request.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ResolveRecord {
    identity: ModuleIdentity,
    resource: PathBuf,
    file_dependencies: BTreeSet<PathBuf>,
    context_dependencies: BTreeSet<PathBuf>,
    missing_dependencies: BTreeSet<PathBuf>,
    snapshot: Snapshot,
}

impl ResolveRecord {
    pub(crate) async fn new(
        identity: ModuleIdentity,
        resource: PathBuf,
        file_dependencies: BTreeSet<PathBuf>,
        context_dependencies: BTreeSet<PathBuf>,
        missing_dependencies: BTreeSet<PathBuf>,
        file_system_info: &FileSystemInfo,
        strategy: SnapshotStrategy,
    ) -> crate::Result<Self> {
        let snapshot = file_system_info
            .create_resolve_snapshot(
                file_dependencies.iter().cloned(),
                context_dependencies.iter().cloned(),
                missing_dependencies.iter().cloned(),
                strategy,
            )
            .await?;
        Ok(Self {
            identity,
            resource,
            file_dependencies,
            context_dependencies,
            missing_dependencies,
            snapshot,
        })
    }

    pub(crate) fn identity(&self) -> &ModuleIdentity {
        &self.identity
    }

    pub(crate) fn resource(&self) -> &Path {
        &self.resource
    }

    pub(crate) fn file_dependencies(&self) -> &BTreeSet<PathBuf> {
        &self.file_dependencies
    }

    pub(crate) fn context_dependencies(&self) -> &BTreeSet<PathBuf> {
        &self.context_dependencies
    }

    pub(crate) fn missing_dependencies(&self) -> &BTreeSet<PathBuf> {
        &self.missing_dependencies
    }

    pub(crate) async fn is_valid(
        &self,
        file_system_info: &FileSystemInfo,
        strategy: SnapshotStrategy,
    ) -> bool {
        file_system_info
            .is_snapshot_valid(&self.snapshot, strategy)
            .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ModuleBuildRecord {
    parsed: ParsedModule,
    source: String,
    snapshot: Snapshot,
}

impl ModuleBuildRecord {
    pub(crate) fn new(parsed: ParsedModule, source: String, snapshot: Snapshot) -> Self {
        Self {
            parsed,
            source,
            snapshot,
        }
    }

    pub(crate) fn parsed(&self) -> &ParsedModule {
        &self.parsed
    }

    pub(crate) fn cloned_parts(&self) -> (ParsedModule, String) {
        (self.parsed.clone(), self.source.clone())
    }

    pub(crate) async fn is_valid(
        &self,
        file_system_info: &FileSystemInfo,
        strategy: SnapshotStrategy,
    ) -> bool {
        file_system_info
            .is_snapshot_valid(&self.snapshot, strategy)
            .await
    }
}

impl BuildCache {
    pub(crate) fn new(options: CacheOptions, snapshot_options: SnapshotOptions) -> Self {
        let build_dependency_snapshot_strategy = snapshot_options.build_dependencies;
        let resolve_build_dependency_snapshot_strategy =
            snapshot_options.resolve_build_dependencies;
        let cache = Self {
            options,
            build_dependency_snapshot_strategy,
            resolve_build_dependency_snapshot_strategy,
            file_system_info: FileSystemInfo::from_snapshot_options(&snapshot_options),
            inner: Arc::new(Mutex::new(BuildCacheInner::default())),
        };
        cache.restore_from_filesystem();
        cache
    }

    pub(crate) fn normal_module_factory(&self) -> NormalModuleFactoryCache {
        CacheFacade {
            build_cache: self.clone(),
            store: resolve_records,
        }
    }

    pub(crate) fn module_builds(&self) -> ModuleBuildCache {
        CacheFacade {
            build_cache: self.clone(),
            store: module_builds,
        }
    }

    pub(crate) fn flush_to_filesystem(&self) -> io::Result<()> {
        if self.options.kind != CacheKind::Filesystem {
            return Ok(());
        }

        let (resolve_records, module_builds) = {
            let inner = self
                .inner
                .lock()
                .expect("build cache mutex should not be poisoned");
            if !inner.dirty {
                return Ok(());
            }
            (
                inner.resolve_records.records.clone(),
                inner.module_builds.records.clone(),
            )
        };

        self.write_filesystem_cache(&resolve_records, &module_builds)?;

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
            resolve_entries: inner.resolve_records.entries(),
            resolve_hits: inner.resolve_records.hits,
            resolve_misses: inner.resolve_records.misses,
            module_entries: inner.module_builds.entries(),
            module_hits: inner.module_builds.hits,
            module_misses: inner.module_builds.misses,
        }
    }
}

impl<K, V> CacheFacade<K, V>
where
    K: Eq + Hash,
{
    pub(crate) fn is_enabled(&self) -> bool {
        self.build_cache.options.kind != CacheKind::Disabled
    }

    pub(crate) fn get(&self, key: &K) -> Option<Arc<V>> {
        if !self.is_enabled() {
            return None;
        }

        let mut inner = self
            .build_cache
            .inner
            .lock()
            .expect("build cache mutex should not be poisoned");
        (self.store)(&mut inner).get(key)
    }

    pub(crate) fn store(&self, key: K, value: V) {
        if !self.is_enabled() {
            return;
        }

        let mut inner = self
            .build_cache
            .inner
            .lock()
            .expect("build cache mutex should not be poisoned");
        (self.store)(&mut inner).store(key, value);
        if self.build_cache.options.kind == CacheKind::Filesystem {
            inner.dirty = true;
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
        if pack.magic != CACHE_MAGIC || pack.cache_version != self.cache_version() {
            return;
        }

        let mut inner = self
            .inner
            .lock()
            .expect("build cache mutex should not be poisoned");
        inner.resolve_records.records = pack
            .resolve_records
            .into_iter()
            .map(|(request, record)| (request, Arc::new(record)))
            .collect();
        inner.module_builds.records = pack
            .module_builds
            .into_iter()
            .map(|(identity, record)| (identity, Arc::new(record)))
            .collect();
    }

    fn write_filesystem_cache(
        &self,
        resolve_records: &HashMap<ResolveRequest, Arc<ResolveRecord>>,
        module_builds: &HashMap<ModuleIdentity, Arc<ModuleBuildRecord>>,
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
            cache_version: cache_version.clone(),
            resolve_records: resolve_records
                .iter()
                .map(|(request, record)| (request.clone(), (**record).clone()))
                .collect(),
            module_builds: module_builds
                .iter()
                .map(|(identity, record)| (identity.clone(), (**record).clone()))
                .collect(),
        };
        let pack_payload = cbor4ii::serde::to_vec(Vec::new(), &pack)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut pack_bytes = PACK_MAGIC.to_vec();
        pack_bytes.extend(pack_payload);
        fs::write(pack_path, pack_bytes)?;

        let manifest = CacheManifest {
            magic: CACHE_MAGIC.to_string(),
            cache_version,
            pack_file: pack_file.to_string_lossy().replace('\\', "/"),
            build_dependencies: self
                .build_dependency_snapshot(self.build_dependency_snapshot_strategy)?,
            resolve_build_dependencies: self
                .build_dependency_snapshot(self.resolve_build_dependency_snapshot_strategy)?,
        };
        fs::create_dir_all(cache_location)?;
        let manifest_json = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(cache_location.join(MANIFEST_FILE), manifest_json)?;
        Ok(())
    }

    fn manifest_is_valid(&self, manifest: &CacheManifest) -> bool {
        manifest.magic == CACHE_MAGIC
            && manifest.cache_version == self.cache_version()
            && self.build_dependency_snapshot_is_valid(
                &manifest.build_dependencies,
                self.build_dependency_snapshot_strategy,
            )
            && self.build_dependency_snapshot_is_valid(
                &manifest.resolve_build_dependencies,
                self.resolve_build_dependency_snapshot_strategy,
            )
    }

    fn cache_version(&self) -> String {
        self.options.version.clone().unwrap_or_default()
    }

    fn build_dependency_snapshot(&self, strategy: SnapshotStrategy) -> io::Result<Snapshot> {
        let snapshots = self
            .options
            .build_dependencies
            .iter()
            .map(|dependency| {
                self.file_system_info
                    .create_snapshot_sync(dependency.files.clone(), strategy)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
            })
            .collect::<io::Result<Vec<_>>>()?;

        Ok(self.file_system_info.merge_snapshots(snapshots.iter()))
    }

    fn build_dependency_snapshot_is_valid(
        &self,
        snapshot: &Snapshot,
        strategy: SnapshotStrategy,
    ) -> bool {
        snapshot.has_exact_paths(self.build_dependency_paths())
            && self
                .file_system_info
                .is_snapshot_valid_sync(snapshot, strategy)
    }

    fn build_dependency_paths(&self) -> Vec<PathBuf> {
        self.options
            .build_dependencies
            .iter()
            .flat_map(|dependency| dependency.files.iter().cloned())
            .collect()
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
    cache_version: String,
    pack_file: String,
    build_dependencies: Snapshot,
    resolve_build_dependencies: Snapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachePackDto {
    magic: String,
    cache_version: String,
    resolve_records: Vec<(ResolveRequest, ResolveRecord)>,
    module_builds: Vec<(ModuleIdentity, ModuleBuildRecord)>,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuildCacheStats {
    pub resolve_entries: usize,
    pub resolve_hits: usize,
    pub resolve_misses: usize,
    pub module_entries: usize,
    pub module_hits: usize,
    pub module_misses: usize,
}

fn resolve_records(inner: &mut BuildCacheInner) -> &mut CacheStore<ResolveRequest, ResolveRecord> {
    &mut inner.resolve_records
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs, path::Path};

    use filetime::{FileTime, set_file_mtime};
    use tempfile::tempdir;

    use super::*;
    use crate::{ModuleIdentity, snapshot::FileSystemInfo};

    #[tokio::test]
    async fn resolve_record_context_snapshot_invalidates_directory_entry_changes()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let context = temp.path().join("src");
        let resource = context.join("dep.js");
        write(&resource, "export const value = 'js';")?;
        let original_mtime = FileTime::from_system_time(fs::metadata(&context)?.modified()?);
        let file_system_info = FileSystemInfo::new();
        let record = ResolveRecord::new(
            ModuleIdentity::new(resource.clone()),
            resource,
            BTreeSet::new(),
            BTreeSet::from([context.clone()]),
            BTreeSet::new(),
            &file_system_info,
            SnapshotStrategy::timestamp(),
        )
        .await?;

        write(context.join("dep.ts"), "export const value = 'ts';")?;
        set_file_mtime(&context, original_mtime)?;

        assert!(
            !record
                .is_valid(&file_system_info, SnapshotStrategy::timestamp())
                .await
        );

        Ok(())
    }

    fn write(path: impl AsRef<Path>, source: &str) -> io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, source)
    }
}

fn module_builds(
    inner: &mut BuildCacheInner,
) -> &mut CacheStore<ModuleIdentity, ModuleBuildRecord> {
    &mut inner.module_builds
}
