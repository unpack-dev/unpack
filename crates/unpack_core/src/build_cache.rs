use std::{
    any::Any,
    collections::{BTreeSet, HashMap},
    fmt, fs, io,
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{
    ModuleIdentity, SnapshotOptions, SnapshotStrategy,
    cache_hash::stable_hash,
    parser::ParsedModule,
    snapshot::{FileSystemInfo, Snapshot, SnapshotCache},
};
use serde::{Deserialize, Serialize};

const CACHE_MAGIC: &str = "UNPACK_PERSISTENT_CACHE";
const PACK_MAGIC: &[u8] = b"UNPACK-CACHE-PACK\0";
const DEFAULT_PACK_FILE: &str = "packs/modules.cbor";
const MANIFEST_FILE: &str = "container.json";
const RESOLVE_CACHE_NAMESPACE: CacheNamespace = CacheNamespace::new("unpack/resolve");
const MODULE_BUILD_CACHE_NAMESPACE: CacheNamespace = CacheNamespace::new("unpack/module-build");

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
    namespace: CacheNamespace,
    family: CacheItemFamily,
    marker: PhantomData<fn(K) -> V>,
}

pub(crate) type NormalModuleFactoryCache = CacheFacade<ResolveRequest, ResolveRecord>;
pub(crate) type ModuleBuildCache = CacheFacade<ModuleIdentity, ModuleBuildRecord>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CacheNamespace(&'static str);

impl CacheNamespace {
    pub(crate) const fn new(value: &'static str) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CacheIdentifier(Vec<u8>);

impl CacheIdentifier {
    #[allow(dead_code)]
    pub(crate) fn new(value: impl AsRef<[u8]>) -> Self {
        Self(value.as_ref().to_vec())
    }

    fn from_parts(parts: impl IntoIterator<Item = Vec<u8>>) -> Self {
        let mut bytes = Vec::new();
        for part in parts {
            bytes.extend_from_slice(&(part.len() as u64).to_le_bytes());
            bytes.extend_from_slice(&part);
        }
        Self(bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CacheETag(Vec<u8>);

impl CacheETag {
    #[allow(dead_code)]
    pub(crate) fn new(value: impl AsRef<[u8]>) -> Self {
        Self(value.as_ref().to_vec())
    }
}

pub(crate) trait CacheKey: Clone + Send + Sync + 'static {
    fn cache_identifier(&self) -> CacheIdentifier;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CacheItemFamily {
    Resolve,
    ModuleBuild,
    #[allow(dead_code)]
    CodeGeneration,
    #[allow(dead_code)]
    AssetRender,
}

impl CacheItemFamily {
    #[cfg(test)]
    const fn index(self) -> usize {
        match self {
            Self::Resolve => 0,
            Self::ModuleBuild => 1,
            Self::CodeGeneration => 2,
            Self::AssetRender => 3,
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CacheItemWork {
    pub hits: usize,
    pub misses: usize,
    pub stores: usize,
    pub restores: usize,
    pub evictions: usize,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CacheWorkCounters {
    by_family: [CacheItemWork; 4],
}

#[cfg(test)]
impl CacheWorkCounters {
    pub(crate) fn for_family(self, family: CacheItemFamily) -> CacheItemWork {
        self.by_family[family.index()]
    }

    fn for_family_mut(&mut self, family: CacheItemFamily) -> &mut CacheItemWork {
        &mut self.by_family[family.index()]
    }
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
    pub readonly: bool,
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
            readonly: false,
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
            readonly: false,
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
            readonly: false,
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

#[derive(Debug)]
struct BuildCacheInner {
    cache: Cache,
    filesystem_manifest: Option<CacheManifest>,
    records_restored: bool,
    dirty: bool,
}

impl BuildCacheInner {
    fn new(options: &CacheOptions) -> Self {
        Self {
            cache: Cache::from_options(options),
            filesystem_manifest: None,
            records_restored: false,
            dirty: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheAddress {
    namespace: CacheNamespace,
    identifier: CacheIdentifier,
}

#[derive(Clone)]
struct CacheEntry {
    family: CacheItemFamily,
    etag: Option<CacheETag>,
    source_key: Arc<dyn Any + Send + Sync>,
    value: Arc<dyn Any + Send + Sync>,
}

impl CacheEntry {
    fn new<K, V>(family: CacheItemFamily, etag: Option<CacheETag>, key: K, value: V) -> Self
    where
        K: Send + Sync + 'static,
        V: Send + Sync + 'static,
    {
        Self {
            family,
            etag,
            source_key: Arc::new(key),
            value: Arc::new(value),
        }
    }

    fn key<K: Send + Sync + 'static>(&self) -> Option<&K> {
        self.source_key.downcast_ref::<K>()
    }

    fn value<V: Send + Sync + 'static>(&self) -> Option<Arc<V>> {
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

trait CacheLayer: fmt::Debug + Send + Sync {
    fn get(&self, address: &CacheAddress, etag: Option<&CacheETag>) -> Option<CacheEntry>;
    fn store(&mut self, address: CacheAddress, entry: CacheEntry);
    #[allow(dead_code)]
    fn evict(&mut self, address: &CacheAddress) -> bool;
    fn entries(&self) -> Vec<(CacheAddress, CacheEntry)>;
}

#[derive(Debug, Default)]
struct MemoryCacheLayer {
    entries: HashMap<CacheAddress, CacheEntry>,
}

impl CacheLayer for MemoryCacheLayer {
    fn get(&self, address: &CacheAddress, etag: Option<&CacheETag>) -> Option<CacheEntry> {
        self.entries
            .get(address)
            .filter(|entry| entry.etag.as_ref() == etag)
            .cloned()
    }

    fn store(&mut self, address: CacheAddress, entry: CacheEntry) {
        self.entries.insert(address, entry);
    }

    fn evict(&mut self, address: &CacheAddress) -> bool {
        self.entries.remove(address).is_some()
    }

    fn entries(&self) -> Vec<(CacheAddress, CacheEntry)> {
        self.entries
            .iter()
            .map(|(address, entry)| (address.clone(), entry.clone()))
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheLayerKind {
    Memory,
    Persistent,
}

#[derive(Debug)]
struct CacheLayerSlot {
    kind: CacheLayerKind,
    writable: bool,
    layer: Box<dyn CacheLayer>,
}

#[derive(Debug)]
struct Cache {
    layers: Vec<CacheLayerSlot>,
    #[cfg(test)]
    work: CacheWorkCounters,
}

impl Cache {
    fn from_options(options: &CacheOptions) -> Self {
        let mut layers = Vec::new();
        if options.kind != CacheKind::Disabled {
            layers.push(CacheLayerSlot {
                kind: CacheLayerKind::Memory,
                writable: true,
                layer: Box::<MemoryCacheLayer>::default(),
            });
        }
        if options.kind == CacheKind::Filesystem {
            layers.push(CacheLayerSlot {
                kind: CacheLayerKind::Persistent,
                writable: !options.readonly,
                layer: Box::<MemoryCacheLayer>::default(),
            });
        }
        Self {
            layers,
            #[cfg(test)]
            work: CacheWorkCounters::default(),
        }
    }

    fn get<V>(
        &mut self,
        _family: CacheItemFamily,
        address: &CacheAddress,
        etag: Option<&CacheETag>,
    ) -> Option<Arc<V>>
    where
        V: Send + Sync + 'static,
    {
        for index in 0..self.layers.len() {
            let entry = self.layers[index].layer.get(address, etag);
            let Some(entry) = entry else {
                continue;
            };
            let Some(value) = entry.value::<V>() else {
                continue;
            };

            if index > 0 {
                for earlier in &mut self.layers[..index] {
                    if earlier.writable {
                        earlier.layer.store(address.clone(), entry.clone());
                    }
                }
                #[cfg(test)]
                {
                    self.work.for_family_mut(_family).restores += 1;
                }
            }
            #[cfg(test)]
            {
                self.work.for_family_mut(_family).hits += 1;
            }
            return Some(value);
        }

        #[cfg(test)]
        {
            self.work.for_family_mut(_family).misses += 1;
        }
        None
    }

    fn store<K, V>(
        &mut self,
        family: CacheItemFamily,
        address: CacheAddress,
        etag: Option<CacheETag>,
        key: K,
        value: V,
    ) -> bool
    where
        K: Send + Sync + 'static,
        V: Send + Sync + 'static,
    {
        let entry = CacheEntry::new(family, etag, key, value);
        let mut stored_persistently = false;
        for slot in &mut self.layers {
            if !slot.writable {
                continue;
            }
            slot.layer.store(address.clone(), entry.clone());
            stored_persistently |= slot.kind == CacheLayerKind::Persistent;
        }
        #[cfg(test)]
        {
            self.work.for_family_mut(family).stores += 1;
        }
        stored_persistently
    }

    fn restore<K, V>(
        &mut self,
        family: CacheItemFamily,
        namespace: CacheNamespace,
        identifier: CacheIdentifier,
        etag: Option<CacheETag>,
        key: K,
        value: V,
    ) where
        K: Send + Sync + 'static,
        V: Send + Sync + 'static,
    {
        let address = CacheAddress {
            namespace,
            identifier,
        };
        let entry = CacheEntry::new(family, etag, key, value);
        if let Some(persistent) = self
            .layers
            .iter_mut()
            .find(|slot| slot.kind == CacheLayerKind::Persistent)
        {
            persistent.layer.store(address, entry);
        }
    }

    fn persistent_entries(&self) -> Vec<(CacheAddress, CacheEntry)> {
        self.layers
            .iter()
            .find(|slot| slot.kind == CacheLayerKind::Persistent)
            .map(|slot| slot.layer.entries())
            .unwrap_or_default()
    }

    #[cfg(test)]
    fn evict_memory(&mut self, family: CacheItemFamily, address: &CacheAddress) {
        let evicted = self
            .layers
            .iter_mut()
            .filter(|slot| slot.kind == CacheLayerKind::Memory)
            .any(|slot| slot.layer.evict(address));
        if evicted {
            self.work.for_family_mut(family).evictions += 1;
        }
    }

    #[cfg(test)]
    fn entry_count(&self, family: CacheItemFamily) -> usize {
        self.layers
            .iter()
            .flat_map(|slot| slot.layer.entries())
            .filter_map(|(address, entry)| (entry.family == family).then_some(address))
            .collect::<std::collections::HashSet<_>>()
            .len()
    }

    #[cfg(test)]
    fn work_counters(&self) -> CacheWorkCounters {
        self.work
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

impl CacheKey for ResolveRequest {
    fn cache_identifier(&self) -> CacheIdentifier {
        CacheIdentifier::from_parts([
            self.context.as_os_str().as_encoded_bytes().to_vec(),
            self.request.as_bytes().to_vec(),
        ])
    }
}

impl CacheKey for ModuleIdentity {
    fn cache_identifier(&self) -> CacheIdentifier {
        let mut parts = vec![
            match self.module_type {
                crate::ModuleType::JavaScriptAuto => b"javascript/auto".to_vec(),
            },
            self.resource.as_os_str().as_encoded_bytes().to_vec(),
            optional_identifier_part(self.query.as_deref()),
            optional_identifier_part(self.fragment.as_deref()),
            optional_identifier_part(self.layer.as_deref()),
            (self.loaders.len() as u64).to_le_bytes().to_vec(),
        ];
        parts.extend(self.loaders.iter().map(|loader| loader.as_bytes().to_vec()));
        CacheIdentifier::from_parts(parts)
    }
}

fn optional_identifier_part(value: Option<&str>) -> Vec<u8> {
    match value {
        Some(value) => {
            let mut bytes = Vec::with_capacity(value.len() + 1);
            bytes.push(1);
            bytes.extend_from_slice(value.as_bytes());
            bytes
        }
        None => vec![0],
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
    pub(crate) fn to_pack_file_dto(&self) -> crate::pack_file::ResolveRecordDto {
        use crate::pack_file::{ModuleIdentityDto, ModuleTypeDto, PathBytes, ResolveRecordDto};

        ResolveRecordDto {
            identity: ModuleIdentityDto {
                module_type: match self.identity.module_type {
                    crate::ModuleType::JavaScriptAuto => ModuleTypeDto::JavaScriptAuto,
                },
                resource: PathBytes::from_path(&self.identity.resource),
                query: self.identity.query.clone(),
                fragment: self.identity.fragment.clone(),
                layer: self.identity.layer.clone(),
                loaders: self.identity.loaders.clone(),
            },
            resource: PathBytes::from_path(&self.resource),
            file_dependencies: self
                .file_dependencies
                .iter()
                .map(|path| PathBytes::from_path(path))
                .collect(),
            context_dependencies: self
                .context_dependencies
                .iter()
                .map(|path| PathBytes::from_path(path))
                .collect(),
            missing_dependencies: self
                .missing_dependencies
                .iter()
                .map(|path| PathBytes::from_path(path))
                .collect(),
            snapshot: self.snapshot.to_pack_file_dto(),
        }
    }

    pub(crate) fn from_pack_file_dto(dto: crate::pack_file::ResolveRecordDto) -> Option<Self> {
        use crate::pack_file::{ModuleTypeDto, ResolveRecordDto};

        let ResolveRecordDto {
            identity,
            resource,
            file_dependencies,
            context_dependencies,
            missing_dependencies,
            snapshot,
        } = dto;
        Some(Self {
            identity: ModuleIdentity {
                module_type: match identity.module_type {
                    ModuleTypeDto::JavaScriptAuto => crate::ModuleType::JavaScriptAuto,
                },
                resource: identity.resource.to_path_buf(),
                query: identity.query,
                fragment: identity.fragment,
                layer: identity.layer,
                loaders: identity.loaders,
            },
            resource: resource.to_path_buf(),
            file_dependencies: file_dependencies
                .into_iter()
                .map(|path| path.to_path_buf())
                .collect(),
            context_dependencies: context_dependencies
                .into_iter()
                .map(|path| path.to_path_buf())
                .collect(),
            missing_dependencies: missing_dependencies
                .into_iter()
                .map(|path| path.to_path_buf())
                .collect(),
            snapshot: Snapshot::from_pack_file_dto(snapshot)?,
        })
    }

    #[cfg(test)]
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

    pub(crate) async fn new_with_cache(
        identity: ModuleIdentity,
        resource: PathBuf,
        file_dependencies: BTreeSet<PathBuf>,
        context_dependencies: BTreeSet<PathBuf>,
        missing_dependencies: BTreeSet<PathBuf>,
        file_system_info: &FileSystemInfo,
        strategy: SnapshotStrategy,
        snapshot_cache: &SnapshotCache,
    ) -> crate::Result<Self> {
        let snapshot = file_system_info
            .create_resolve_snapshot_with_cache(
                file_dependencies.iter().cloned(),
                context_dependencies.iter().cloned(),
                missing_dependencies.iter().cloned(),
                strategy,
                snapshot_cache,
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

    #[cfg(test)]
    pub(crate) async fn is_valid(
        &self,
        file_system_info: &FileSystemInfo,
        strategy: SnapshotStrategy,
    ) -> bool {
        file_system_info
            .is_snapshot_valid(&self.snapshot, strategy)
            .await
    }

    pub(crate) async fn is_valid_with_cache(
        &self,
        file_system_info: &FileSystemInfo,
        strategy: SnapshotStrategy,
        snapshot_cache: &SnapshotCache,
    ) -> bool {
        file_system_info
            .is_snapshot_valid_with_cache(&self.snapshot, strategy, snapshot_cache)
            .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ModuleBuildRecord {
    parsed: ParsedModule,
    source: String,
    #[serde(default)]
    source_hash: Option<u64>,
    snapshot: Snapshot,
}

impl ModuleBuildRecord {
    pub(crate) fn new(parsed: ParsedModule, source: String, snapshot: Snapshot) -> Self {
        let source_hash = Some(stable_hash(&source));
        Self {
            parsed,
            source,
            source_hash,
            snapshot,
        }
    }

    pub(crate) fn parsed(&self) -> &ParsedModule {
        &self.parsed
    }

    pub(crate) fn cloned_parts(&self) -> (ParsedModule, String, Option<u64>) {
        (self.parsed.clone(), self.source.clone(), self.source_hash)
    }

    pub(crate) fn source_hash(&self) -> Option<u64> {
        self.source_hash
    }

    pub(crate) async fn is_valid_with_cache(
        &self,
        file_system_info: &FileSystemInfo,
        strategy: SnapshotStrategy,
        snapshot_cache: &SnapshotCache,
    ) -> bool {
        file_system_info
            .is_snapshot_valid_with_cache(&self.snapshot, strategy, snapshot_cache)
            .await
    }
}

impl BuildCache {
    pub(crate) fn new(options: CacheOptions, snapshot_options: SnapshotOptions) -> Self {
        let build_dependency_snapshot_strategy = snapshot_options.build_dependencies;
        let resolve_build_dependency_snapshot_strategy =
            snapshot_options.resolve_build_dependencies;
        let inner = BuildCacheInner::new(&options);
        let cache = Self {
            options,
            build_dependency_snapshot_strategy,
            resolve_build_dependency_snapshot_strategy,
            file_system_info: FileSystemInfo::from_snapshot_options(&snapshot_options),
            inner: Arc::new(Mutex::new(inner)),
        };
        cache.restore_from_filesystem();
        cache
    }

    pub(crate) fn normal_module_factory(&self) -> NormalModuleFactoryCache {
        self.facade(RESOLVE_CACHE_NAMESPACE, CacheItemFamily::Resolve)
    }

    pub(crate) fn module_builds(&self) -> ModuleBuildCache {
        self.facade(MODULE_BUILD_CACHE_NAMESPACE, CacheItemFamily::ModuleBuild)
    }

    fn facade<K, V>(
        &self,
        namespace: CacheNamespace,
        family: CacheItemFamily,
    ) -> CacheFacade<K, V> {
        CacheFacade {
            build_cache: self.clone(),
            namespace,
            family,
            marker: PhantomData,
        }
    }

    pub(crate) fn flush_to_filesystem(&self) -> io::Result<()> {
        if self.options.kind != CacheKind::Filesystem || self.options.readonly {
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
            legacy_filesystem_records(&inner.cache)
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
        let work = inner.cache.work_counters();
        let resolve = work.for_family(CacheItemFamily::Resolve);
        let module = work.for_family(CacheItemFamily::ModuleBuild);
        BuildCacheStats {
            resolve_entries: inner.cache.entry_count(CacheItemFamily::Resolve),
            resolve_hits: resolve.hits,
            resolve_misses: resolve.misses,
            module_entries: inner.cache.entry_count(CacheItemFamily::ModuleBuild),
            module_hits: module.hits,
            module_misses: module.misses,
        }
    }

    #[cfg(test)]
    pub(crate) fn work_counters(&self) -> CacheWorkCounters {
        self.inner
            .lock()
            .expect("build cache mutex should not be poisoned")
            .cache
            .work_counters()
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

        self.build_cache.restore_records_from_filesystem_if_needed();

        let mut inner = self
            .build_cache
            .inner
            .lock()
            .expect("build cache mutex should not be poisoned");
        let address = CacheAddress {
            namespace: self.namespace,
            identifier: key.cache_identifier(),
        };
        inner.cache.get(self.family, &address, etag)
    }

    pub(crate) fn store(&self, key: K, etag: Option<CacheETag>, value: V) {
        if !self.is_enabled() {
            return;
        }

        let mut inner = self
            .build_cache
            .inner
            .lock()
            .expect("build cache mutex should not be poisoned");
        let address = CacheAddress {
            namespace: self.namespace,
            identifier: key.cache_identifier(),
        };
        if inner.cache.store(self.family, address, etag, key, value) {
            inner.dirty = true;
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
            .lock()
            .expect("build cache mutex should not be poisoned")
            .cache
            .evict_memory(self.family, &address);
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

        let mut inner = self
            .inner
            .lock()
            .expect("build cache mutex should not be poisoned");
        inner.filesystem_manifest = Some(manifest.clone());
        drop(inner);

        if !self.options.readonly {
            self.restore_records_from_filesystem_if_needed();
        }
    }

    fn restore_records_from_filesystem_if_needed(&self) {
        if self.options.kind != CacheKind::Filesystem {
            return;
        }
        let Some(cache_location) = &self.options.cache_location else {
            self.mark_records_restored();
            return;
        };
        let manifest = {
            let inner = self
                .inner
                .lock()
                .expect("build cache mutex should not be poisoned");
            if inner.records_restored {
                return;
            }
            inner.filesystem_manifest.clone()
        };
        let Some(manifest) = manifest else {
            self.mark_records_restored();
            return;
        };

        let Some(pack) = read_pack(cache_location, &manifest.pack_file) else {
            self.mark_records_restored();
            return;
        };
        if pack.magic != CACHE_MAGIC || pack.cache_version != self.cache_version() {
            self.mark_records_restored();
            return;
        }

        let mut inner = self
            .inner
            .lock()
            .expect("build cache mutex should not be poisoned");
        for (request, record) in pack.resolve_records {
            inner.cache.restore(
                CacheItemFamily::Resolve,
                RESOLVE_CACHE_NAMESPACE,
                request.cache_identifier(),
                None,
                request,
                record,
            );
        }
        for (identity, record) in pack.module_builds {
            inner.cache.restore(
                CacheItemFamily::ModuleBuild,
                MODULE_BUILD_CACHE_NAMESPACE,
                identity.cache_identifier(),
                None,
                identity,
                record,
            );
        }
        inner.records_restored = true;
    }

    fn mark_records_restored(&self) {
        self.inner
            .lock()
            .expect("build cache mutex should not be poisoned")
            .records_restored = true;
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

fn legacy_filesystem_records(
    cache: &Cache,
) -> (
    HashMap<ResolveRequest, Arc<ResolveRecord>>,
    HashMap<ModuleIdentity, Arc<ModuleBuildRecord>>,
) {
    let mut resolve_records = HashMap::new();
    let mut module_builds = HashMap::new();
    for (_, entry) in cache.persistent_entries() {
        match entry.family {
            CacheItemFamily::Resolve => {
                if let (Some(key), Some(value)) = (
                    entry.key::<ResolveRequest>(),
                    entry.value::<ResolveRecord>(),
                ) {
                    resolve_records.insert(key.clone(), value);
                }
            }
            CacheItemFamily::ModuleBuild => {
                if let (Some(key), Some(value)) = (
                    entry.key::<ModuleIdentity>(),
                    entry.value::<ModuleBuildRecord>(),
                ) {
                    module_builds.insert(key.clone(), value);
                }
            }
            CacheItemFamily::CodeGeneration | CacheItemFamily::AssetRender => {}
        }
    }
    (resolve_records, module_builds)
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

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs, path::Path};

    use filetime::{FileTime, set_file_mtime};
    use tempfile::tempdir;

    use super::*;
    use crate::{ModuleIdentity, snapshot::FileSystemInfo};

    #[derive(Debug, Clone)]
    struct TestCacheKey(&'static str);

    impl CacheKey for TestCacheKey {
        fn cache_identifier(&self) -> CacheIdentifier {
            CacheIdentifier::new(self.0)
        }
    }

    #[test]
    fn cache_facades_scope_identical_identifiers_by_namespace_and_etag() {
        let build_cache = BuildCache::new(CacheOptions::memory(), SnapshotOptions::default());
        let code_generation = build_cache.facade::<TestCacheKey, String>(
            CacheNamespace::new("unpack/code-generation"),
            CacheItemFamily::CodeGeneration,
        );
        let asset_render = build_cache.facade::<TestCacheKey, String>(
            CacheNamespace::new("unpack/asset-render"),
            CacheItemFamily::AssetRender,
        );
        assert_eq!(
            code_generation.namespace(),
            CacheNamespace::new("unpack/code-generation")
        );
        assert_eq!(
            asset_render.namespace(),
            CacheNamespace::new("unpack/asset-render")
        );
        let identifier = TestCacheKey("shared-identifier");
        let current = CacheETag::new("current");
        let stale = CacheETag::new("stale");

        code_generation.store(
            identifier.clone(),
            Some(current.clone()),
            "generated source".to_string(),
        );
        asset_render.store(
            identifier.clone(),
            Some(current.clone()),
            "rendered asset".to_string(),
        );

        assert_eq!(
            code_generation
                .get(&identifier, Some(&current))
                .as_deref()
                .map(String::as_str),
            Some("generated source")
        );
        assert_eq!(
            asset_render
                .get(&identifier, Some(&current))
                .as_deref()
                .map(String::as_str),
            Some("rendered asset")
        );
        assert!(code_generation.get(&identifier, Some(&stale)).is_none());

        let counters = build_cache.work_counters();
        assert_eq!(
            counters.for_family(CacheItemFamily::CodeGeneration),
            CacheItemWork {
                hits: 1,
                misses: 1,
                stores: 1,
                restores: 0,
                evictions: 0,
            }
        );
        assert_eq!(
            counters.for_family(CacheItemFamily::AssetRender),
            CacheItemWork {
                hits: 1,
                misses: 0,
                stores: 1,
                restores: 0,
                evictions: 0,
            }
        );
    }

    #[test]
    fn cache_facade_accounts_for_memory_eviction_by_item_family() {
        let build_cache = BuildCache::new(CacheOptions::memory(), SnapshotOptions::default());
        let code_generation = build_cache.facade::<TestCacheKey, String>(
            CacheNamespace::new("unpack/code-generation"),
            CacheItemFamily::CodeGeneration,
        );
        let identifier = TestCacheKey("evicted-identifier");

        code_generation.store(identifier.clone(), None, "generated source".to_string());
        code_generation.evict_memory(&identifier);

        assert!(code_generation.get(&identifier, None).is_none());
        assert_eq!(
            build_cache
                .work_counters()
                .for_family(CacheItemFamily::CodeGeneration),
            CacheItemWork {
                hits: 0,
                misses: 1,
                stores: 1,
                restores: 0,
                evictions: 1,
            }
        );
    }

    #[test]
    fn lower_layer_hit_repopulates_the_earlier_memory_cache() {
        let build_cache = BuildCache::new(CacheOptions::filesystem(), SnapshotOptions::default());
        let code_generation = build_cache.facade::<TestCacheKey, String>(
            CacheNamespace::new("unpack/code-generation"),
            CacheItemFamily::CodeGeneration,
        );
        let identifier = TestCacheKey("restored-identifier");

        code_generation.store(identifier.clone(), None, "generated source".to_string());
        code_generation.evict_memory(&identifier);

        assert_eq!(
            code_generation
                .get(&identifier, None)
                .as_deref()
                .map(String::as_str),
            Some("generated source")
        );
        assert_eq!(
            code_generation
                .get(&identifier, None)
                .as_deref()
                .map(String::as_str),
            Some("generated source")
        );
        assert_eq!(
            build_cache
                .work_counters()
                .for_family(CacheItemFamily::CodeGeneration),
            CacheItemWork {
                hits: 2,
                misses: 0,
                stores: 1,
                restores: 1,
                evictions: 1,
            }
        );
    }

    #[test]
    fn module_build_record_without_source_hash_deserializes_with_empty_hash()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let record: ModuleBuildRecord = serde_json::from_value(serde_json::json!({
            "parsed": {
                "dependencies": [],
                "blocks": [],
                "presentational_dependencies": []
            },
            "source": "export const value = 1;",
            "snapshot": {
                "entries": []
            }
        }))?;

        assert_eq!(record.source_hash(), None);

        Ok(())
    }

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
