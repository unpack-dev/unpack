use std::{
    any::Any,
    collections::{BTreeSet, HashMap},
    fmt, io,
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{
    ModuleIdentity, SnapshotOptions, SnapshotStrategy,
    cache_hash::stable_hash,
    pack_file::{
        CodecRegistry, ModuleBuildRecordCodec, ModuleBuildRecordDto, PackFile, PackFileAddress,
        PackFileETag, PackFileGuardDto, PackFileWriteBatch, PublicationBase, ResolveRecordCodec,
        ResolveRecordDto, SnapshotDto,
    },
    parser::ParsedModule,
    snapshot::{FileSystemInfo, Snapshot, SnapshotCache},
};

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

    const fn as_str(self) -> &'static str {
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

    fn from_parts(parts: impl IntoIterator<Item = Vec<u8>>) -> Self {
        let mut bytes = Vec::new();
        for part in parts {
            bytes.extend_from_slice(&(part.len() as u64).to_le_bytes());
            bytes.extend_from_slice(&part);
        }
        Self(bytes)
    }

    fn as_bytes(&self) -> &[u8] {
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
    const fn index(self) -> usize {
        match self {
            Self::Resolve => 0,
            Self::ModuleBuild => 1,
            Self::CodeGeneration => 2,
            Self::AssetRender => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CacheItemWork {
    pub hits: usize,
    pub misses: usize,
    pub stores: usize,
    pub restores: usize,
    pub evictions: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CacheWorkCounters {
    by_family: [CacheItemWork; 4],
}

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
    pub idle_timeout_for_initial_store: Option<u32>,
    pub idle_timeout_after_large_changes: Option<u32>,
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
            idle_timeout_for_initial_store: None,
            idle_timeout_after_large_changes: None,
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
            idle_timeout_for_initial_store: None,
            idle_timeout_after_large_changes: None,
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
            idle_timeout: Some(60_000),
            idle_timeout_for_initial_store: Some(5_000),
            idle_timeout_after_large_changes: Some(1_000),
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
    dirty_generation: u64,
    published_generation: u64,
    initial_store_pending: bool,
    persistent_guard: Option<PackFileGuardDto>,
    persistent_guard_error: Option<String>,
    #[cfg(test)]
    publish_barrier: Option<PublishBarrier>,
}

#[cfg(test)]
#[derive(Debug)]
struct PublishBarrier {
    entered: Arc<std::sync::Barrier>,
    release: Arc<std::sync::Barrier>,
}

impl BuildCacheInner {
    fn new(
        options: &CacheOptions,
        file_system_info: &FileSystemInfo,
        build_dependency_snapshot_strategy: SnapshotStrategy,
        resolve_build_dependency_snapshot_strategy: SnapshotStrategy,
    ) -> Self {
        let cache = Cache::from_options(
            options,
            file_system_info,
            build_dependency_snapshot_strategy,
            resolve_build_dependency_snapshot_strategy,
        );
        let initial_store_pending = !cache.has_persistent_publication();
        Self {
            cache,
            dirty_generation: 0,
            published_generation: 0,
            initial_store_pending,
            persistent_guard: None,
            persistent_guard_error: None,
            #[cfg(test)]
            publish_barrier: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheAddress {
    namespace: CacheNamespace,
    identifier: CacheIdentifier,
}

impl CacheAddress {
    fn to_pack_file_address(&self) -> PackFileAddress {
        PackFileAddress::new(self.namespace.as_str(), self.identifier.as_bytes())
    }
}

impl CacheETag {
    fn to_pack_file_etag(&self) -> PackFileETag {
        PackFileETag::new(self.as_bytes())
    }
}

#[derive(Clone)]
struct CacheEntry {
    family: CacheItemFamily,
    etag: Option<CacheETag>,
    value: Arc<dyn Any + Send + Sync>,
}

impl CacheEntry {
    fn new<V>(family: CacheItemFamily, etag: Option<CacheETag>, value: V) -> Self
    where
        V: Send + Sync + 'static,
    {
        Self {
            family,
            etag,
            value: Arc::new(value),
        }
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
    fn get(&mut self, address: &CacheAddress, etag: Option<&CacheETag>) -> Option<CacheEntry>;
    fn store(&mut self, address: CacheAddress, entry: CacheEntry);
    fn publish(&mut self, _guard: &PackFileGuardDto) -> io::Result<()> {
        Ok(())
    }
    fn has_publication(&self) -> bool {
        false
    }
    #[allow(dead_code)]
    fn evict(&mut self, address: &CacheAddress) -> bool;
    #[cfg(test)]
    fn entry_count(&self, family: CacheItemFamily) -> usize;
}

#[derive(Debug, Default)]
struct MemoryCacheLayer {
    entries: HashMap<CacheAddress, CacheEntry>,
}

impl CacheLayer for MemoryCacheLayer {
    fn get(&mut self, address: &CacheAddress, etag: Option<&CacheETag>) -> Option<CacheEntry> {
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

    #[cfg(test)]
    fn entry_count(&self, family: CacheItemFamily) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.family == family)
            .count()
    }
}

#[derive(Debug)]
struct PackFileCacheLayer {
    root: Option<PathBuf>,
    registry: CodecRegistry,
    pack_file: Option<PackFile>,
    publication_base: PublicationBase,
    active: bool,
    pending: HashMap<CacheAddress, CacheEntry>,
}

impl PackFileCacheLayer {
    fn open(
        options: &CacheOptions,
        file_system_info: &FileSystemInfo,
        build_dependency_snapshot_strategy: SnapshotStrategy,
        resolve_build_dependency_snapshot_strategy: SnapshotStrategy,
    ) -> Self {
        let registry = persistent_codec_registry();
        let root = options.cache_location.clone();
        let pack_file = root
            .as_ref()
            .map(|root| PackFile::open(root, registry.clone()));
        let active = pack_file.as_ref().is_some_and(|pack_file| {
            pack_file.guard().is_some_and(|guard| {
                persistent_guard_is_valid(
                    guard,
                    options,
                    file_system_info,
                    build_dependency_snapshot_strategy,
                    resolve_build_dependency_snapshot_strategy,
                )
            })
        });
        let publication_base = if active {
            PublicationBase::PreserveEntries {
                expected_revision: pack_file
                    .as_ref()
                    .expect("active PackFile should be open")
                    .revision(),
            }
        } else {
            PublicationBase::ReplaceAll
        };
        Self {
            root,
            registry,
            pack_file,
            publication_base,
            active,
            pending: HashMap::new(),
        }
    }

    fn publish(&mut self, guard: PackFileGuardDto) -> io::Result<()> {
        let Some(root) = self.root.as_ref() else {
            self.pending.clear();
            return Ok(());
        };
        let mut batch = PackFileWriteBatch::new();
        for (address, entry) in &self.pending {
            let pack_address = address.to_pack_file_address();
            let pack_etag = entry.etag.as_ref().map(CacheETag::to_pack_file_etag);
            match entry.family {
                CacheItemFamily::Resolve => {
                    let record = entry.value::<ResolveRecord>().ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Resolve Cache Item contains an unexpected value",
                        )
                    })?;
                    let dto = ResolveRecordDto::try_from(record.as_ref())?;
                    batch.insert(&self.registry, pack_address, pack_etag, dto)?;
                }
                CacheItemFamily::ModuleBuild => {
                    let record = entry.value::<ModuleBuildRecord>().ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Module Build Cache Item contains an unexpected value",
                        )
                    })?;
                    let dto = ModuleBuildRecordDto::try_from(record.as_ref())?;
                    batch.insert(&self.registry, pack_address, pack_etag, dto)?;
                }
                CacheItemFamily::CodeGeneration | CacheItemFamily::AssetRender => continue,
            }
        }
        PackFile::publish_batch(root, Some(guard), self.publication_base, batch)?;
        self.pending.clear();
        let pack_file = PackFile::open(root, self.registry.clone());
        self.publication_base = PublicationBase::PreserveEntries {
            expected_revision: pack_file.revision(),
        };
        self.pack_file = Some(pack_file);
        self.active = true;
        Ok(())
    }
}

impl CacheLayer for PackFileCacheLayer {
    fn get(&mut self, address: &CacheAddress, etag: Option<&CacheETag>) -> Option<CacheEntry> {
        if let Some(entry) = self
            .pending
            .get(address)
            .filter(|entry| entry.etag.as_ref() == etag)
        {
            return Some(entry.clone());
        }
        if !self.active {
            return None;
        }
        let pack_file = self.pack_file.as_mut()?;
        let pack_address = address.to_pack_file_address();
        let pack_etag = etag.map(CacheETag::to_pack_file_etag);
        match address.namespace {
            RESOLVE_CACHE_NAMESPACE => {
                let dto = pack_file.get_resolve_record(&pack_address, pack_etag.as_ref())?;
                let record = ResolveRecord::try_from((*dto).clone()).ok()?;
                Some(CacheEntry::new(
                    CacheItemFamily::Resolve,
                    etag.cloned(),
                    record,
                ))
            }
            MODULE_BUILD_CACHE_NAMESPACE => {
                let dto = pack_file.get_module_build_record(&pack_address, pack_etag.as_ref())?;
                let record = ModuleBuildRecord::try_from((*dto).clone()).ok()?;
                Some(CacheEntry::new(
                    CacheItemFamily::ModuleBuild,
                    etag.cloned(),
                    record,
                ))
            }
            _ => None,
        }
    }

    fn store(&mut self, address: CacheAddress, entry: CacheEntry) {
        self.pending.insert(address, entry);
    }

    fn publish(&mut self, guard: &PackFileGuardDto) -> io::Result<()> {
        PackFileCacheLayer::publish(self, guard.clone())
    }

    fn has_publication(&self) -> bool {
        self.active
    }

    fn evict(&mut self, address: &CacheAddress) -> bool {
        self.pending.remove(address).is_some()
    }

    #[cfg(test)]
    fn entry_count(&self, family: CacheItemFamily) -> usize {
        self.pending
            .values()
            .filter(|entry| entry.family == family)
            .count()
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
    work: CacheWorkCounters,
}

impl Cache {
    fn from_options(
        options: &CacheOptions,
        file_system_info: &FileSystemInfo,
        build_dependency_snapshot_strategy: SnapshotStrategy,
        resolve_build_dependency_snapshot_strategy: SnapshotStrategy,
    ) -> Self {
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
                layer: Box::new(PackFileCacheLayer::open(
                    options,
                    file_system_info,
                    build_dependency_snapshot_strategy,
                    resolve_build_dependency_snapshot_strategy,
                )),
            });
        }
        Self {
            layers,
            work: CacheWorkCounters::default(),
        }
    }

    fn get<V>(
        &mut self,
        family: CacheItemFamily,
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
                self.work.for_family_mut(family).restores += 1;
            }
            self.work.for_family_mut(family).hits += 1;
            return Some(value);
        }

        self.work.for_family_mut(family).misses += 1;
        None
    }

    fn store<V>(
        &mut self,
        family: CacheItemFamily,
        address: CacheAddress,
        etag: Option<CacheETag>,
        value: V,
    ) -> bool
    where
        V: Send + Sync + 'static,
    {
        let entry = CacheEntry::new(family, etag, value);
        let mut stored_persistently = false;
        for slot in &mut self.layers {
            if !slot.writable {
                continue;
            }
            slot.layer.store(address.clone(), entry.clone());
            stored_persistently |= slot.kind == CacheLayerKind::Persistent;
        }
        self.work.for_family_mut(family).stores += 1;
        stored_persistently
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
            .find(|slot| slot.kind == CacheLayerKind::Memory)
            .map(|slot| slot.layer.entry_count(family))
            .unwrap_or_default()
    }

    fn work_counters(&self) -> CacheWorkCounters {
        self.work
    }

    fn publish_persistent(&mut self, guard: PackFileGuardDto) -> io::Result<()> {
        let Some(slot) = self
            .layers
            .iter_mut()
            .find(|slot| slot.kind == CacheLayerKind::Persistent)
        else {
            return Ok(());
        };
        slot.layer.publish(&guard)
    }

    fn has_persistent_publication(&self) -> bool {
        self.layers
            .iter()
            .find(|slot| slot.kind == CacheLayerKind::Persistent)
            .is_some_and(|slot| slot.layer.has_publication())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolveRecord {
    identity: ModuleIdentity,
    resource: PathBuf,
    file_dependencies: BTreeSet<PathBuf>,
    context_dependencies: BTreeSet<PathBuf>,
    missing_dependencies: BTreeSet<PathBuf>,
    snapshot: Snapshot,
}

impl ResolveRecord {
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

    pub(crate) fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub(crate) fn from_persistent_parts(
        identity: ModuleIdentity,
        resource: PathBuf,
        file_dependencies: BTreeSet<PathBuf>,
        context_dependencies: BTreeSet<PathBuf>,
        missing_dependencies: BTreeSet<PathBuf>,
        snapshot: Snapshot,
    ) -> Self {
        Self {
            identity,
            resource,
            file_dependencies,
            context_dependencies,
            missing_dependencies,
            snapshot,
        }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModuleBuildRecord {
    parsed: ParsedModule,
    source: String,
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

    pub(crate) fn snapshot(&self) -> &Snapshot {
        &self.snapshot
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
        let file_system_info = FileSystemInfo::from_snapshot_options(&snapshot_options);
        let inner = BuildCacheInner::new(
            &options,
            &file_system_info,
            build_dependency_snapshot_strategy,
            resolve_build_dependency_snapshot_strategy,
        );
        Self {
            options,
            build_dependency_snapshot_strategy,
            resolve_build_dependency_snapshot_strategy,
            file_system_info,
            inner: Arc::new(Mutex::new(inner)),
        }
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

    pub(crate) fn store_build_dependencies(&self) {
        if self.options.kind != CacheKind::Filesystem || self.options.readonly {
            return;
        }
        let guard = self
            .current_persistent_guard()
            .map_err(|error| error.to_string());
        let mut inner = self
            .inner
            .lock()
            .expect("build cache mutex should not be poisoned");
        match guard {
            Ok(guard) => {
                inner.persistent_guard = Some(guard);
                inner.persistent_guard_error = None;
            }
            Err(error) => {
                inner.persistent_guard = None;
                inner.persistent_guard_error = Some(error);
            }
        }
    }

    pub(crate) fn pending_generation(&self) -> Option<u64> {
        let inner = self
            .inner
            .lock()
            .expect("build cache mutex should not be poisoned");
        (inner.dirty_generation > inner.published_generation).then_some(inner.dirty_generation)
    }

    pub(crate) fn initial_store_pending(&self) -> bool {
        self.inner
            .lock()
            .expect("build cache mutex should not be poisoned")
            .initial_store_pending
    }

    pub(crate) fn publish_generation(&self, target_generation: u64) -> io::Result<()> {
        if self.options.kind != CacheKind::Filesystem || self.options.readonly {
            return Ok(());
        }
        let mut inner = self
            .inner
            .lock()
            .expect("build cache mutex should not be poisoned");
        if target_generation <= inner.published_generation {
            return Ok(());
        }
        #[cfg(test)]
        if let Some(barrier) = inner.publish_barrier.take() {
            barrier.entered.wait();
            barrier.release.wait();
        }
        if let Some(error) = &inner.persistent_guard_error {
            return Err(io::Error::new(io::ErrorKind::InvalidData, error.clone()));
        }
        let guard = inner.persistent_guard.clone().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Build Dependency state was not stored before cache publication",
            )
        })?;
        inner.cache.publish_persistent(guard)?;
        inner.published_generation = inner.published_generation.max(target_generation);
        inner.initial_store_pending = false;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn install_publish_barrier(
        &self,
        entered: Arc<std::sync::Barrier>,
        release: Arc<std::sync::Barrier>,
    ) {
        self.inner
            .lock()
            .expect("build cache mutex should not be poisoned")
            .publish_barrier = Some(PublishBarrier { entered, release });
    }

    pub(crate) fn flush_to_filesystem(&self) -> io::Result<()> {
        self.store_build_dependencies();
        let Some(target_generation) = self.pending_generation() else {
            return Ok(());
        };
        self.publish_generation(target_generation)
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

    pub(crate) fn work_counters(&self) -> CacheWorkCounters {
        self.inner
            .lock()
            .expect("build cache mutex should not be poisoned")
            .cache
            .work_counters()
    }

    pub(crate) fn trace_work_counters(&self) {
        let work = self.work_counters();
        let resolve = work.for_family(CacheItemFamily::Resolve);
        let module = work.for_family(CacheItemFamily::ModuleBuild);
        tracing::info!(
            target: "unpack_core::cache_work",
            resolve_hits = resolve.hits,
            resolve_misses = resolve.misses,
            resolve_stores = resolve.stores,
            resolve_restores = resolve.restores,
            resolve_evictions = resolve.evictions,
            module_hits = module.hits,
            module_misses = module.misses,
            module_stores = module.stores,
            module_restores = module.restores,
            module_evictions = module.evictions,
            "cache_work"
        );
    }

    fn current_persistent_guard(&self) -> io::Result<PackFileGuardDto> {
        Ok(PackFileGuardDto {
            version: self.cache_version().into_bytes(),
            build_dependencies: SnapshotDto::try_from(
                &self.build_dependency_snapshot(self.build_dependency_snapshot_strategy)?,
            )?,
            resolve_build_dependencies: SnapshotDto::try_from(
                &self.build_dependency_snapshot(self.resolve_build_dependency_snapshot_strategy)?,
            )?,
        })
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
        if inner.cache.store(self.family, address, etag, value) {
            inner.dirty_generation = inner.dirty_generation.saturating_add(1);
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
}

fn persistent_codec_registry() -> CodecRegistry {
    CodecRegistry::new()
        .with_resolve_record(ResolveRecordCodec::current())
        .with_module_build_record(ModuleBuildRecordCodec::current())
}

fn persistent_guard_is_valid(
    guard: &PackFileGuardDto,
    options: &CacheOptions,
    file_system_info: &FileSystemInfo,
    build_dependency_snapshot_strategy: SnapshotStrategy,
    resolve_build_dependency_snapshot_strategy: SnapshotStrategy,
) -> bool {
    if guard.version != options.version.clone().unwrap_or_default().as_bytes() {
        return false;
    }
    let Ok(build_dependencies) = Snapshot::try_from(guard.build_dependencies.clone()) else {
        return false;
    };
    let Ok(resolve_build_dependencies) =
        Snapshot::try_from(guard.resolve_build_dependencies.clone())
    else {
        return false;
    };
    let paths = options
        .build_dependencies
        .iter()
        .flat_map(|dependency| dependency.files.iter().cloned())
        .collect::<Vec<_>>();
    build_dependencies.has_exact_paths(paths.clone())
        && file_system_info
            .is_snapshot_valid_sync(&build_dependencies, build_dependency_snapshot_strategy)
        && resolve_build_dependencies.has_exact_paths(paths)
        && file_system_info.is_snapshot_valid_sync(
            &resolve_build_dependencies,
            resolve_build_dependency_snapshot_strategy,
        )
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
    fn filesystem_cache_uses_pinned_idle_timeout_defaults() {
        let options = CacheOptions::filesystem();
        assert_eq!(options.idle_timeout, Some(60_000));
        assert_eq!(options.idle_timeout_for_initial_store, Some(5_000));
        assert_eq!(options.idle_timeout_after_large_changes, Some(1_000));
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
