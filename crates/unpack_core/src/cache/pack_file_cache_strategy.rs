// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/cache/PackFileCacheStrategy.js

//! Webpack-aligned Pack File Cache Strategy, including restore, publication, and writer diagnostics.

use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{
    SnapshotStrategy,
    cache::pack_file::{
        AccessStamp, AssetRenderRecordCodec, AssetRenderRecordDto, CodeGenerationRecordCodec,
        CodeGenerationRecordDto, ModuleBuildRecordCodec, ModuleBuildRecordDto, PackFileAddress,
        PackFileCompression, PackFileETag, PackFileGuardDto, PackFilePublicationOptions,
        PackFileRetention, PublicationBase, ResolveRecordCodec, ResolveRecordDto,
    },
    code_generation_record::CodeGenerationRecord,
    rendered_source::RenderedSource,
    serialization::Serializer,
    snapshot::{FileSystemInfo, Snapshot},
};
use rustc_hash::FxHashMap;

use super::turbo_persistence_storage::{
    TurboPersistenceRestore, TurboPersistenceStorage, TurboPersistenceWriteBatch,
};

#[cfg(test)]
use super::RestoreBarrier;
use super::{
    CacheDiagnostics, CacheItemFamily,
    cache_items::{ModuleBuildRecord, ResolveRecord},
    cache_layers::{CacheEntry, CacheLayer, CacheLayerLookup},
    options::CacheOptions,
};
use crate::cache_facade::{
    ASSET_RENDER_CACHE_NAMESPACE, CODE_GENERATION_CACHE_NAMESPACE, CacheAddress, CacheETag,
    CacheNamespace, MODULE_BUILD_CACHE_NAMESPACE, RESOLVE_CACHE_NAMESPACE,
};

enum PreparedPersistentRecord {
    Resolve(TurboPersistenceRestore<ResolveRecordDto>),
    ModuleBuild(TurboPersistenceRestore<ModuleBuildRecordDto>),
    CodeGeneration(TurboPersistenceRestore<CodeGenerationRecordDto>),
    AssetRender(TurboPersistenceRestore<AssetRenderRecordDto>),
}

pub(crate) struct PersistentCachePreparation<'a> {
    pub(super) guard: &'a PackFileGuardDto,
    pub(super) build_inputs: &'a BTreeSet<PathBuf>,
    pub(super) resolved_build_inputs: &'a BTreeSet<PathBuf>,
    pub(super) automatic_build_inputs: &'a BTreeSet<PathBuf>,
    pub(super) file_system_info: &'a FileSystemInfo,
    pub(super) build_dependency_snapshot_strategy: SnapshotStrategy,
    pub(super) resolve_build_dependency_snapshot_strategy: SnapshotStrategy,
}

#[derive(Debug, Clone)]
pub(crate) struct PersistentRestore {
    storage: Arc<Mutex<TurboPersistenceStorage>>,
    reader_generation: u64,
    address: PackFileAddress,
    etag: Option<PackFileETag>,
    cache_etag: Option<CacheETag>,
    namespace: CacheNamespace,
    diagnostics: Arc<CacheDiagnostics>,
    #[cfg(test)]
    barrier: Option<RestoreBarrier>,
}

impl PersistentRestore {
    pub(super) fn restore(&self) -> Option<CacheEntry> {
        let started = self.diagnostics.profile_enabled().then(Instant::now);
        let prepared = {
            let storage = self
                .storage
                .lock()
                .expect("Persistent Cache storage mutex should not be poisoned");
            match self.namespace {
                RESOLVE_CACHE_NAMESPACE => PreparedPersistentRecord::Resolve(
                    storage.prepare_restore(&self.address, self.etag.as_ref())?,
                ),
                MODULE_BUILD_CACHE_NAMESPACE => PreparedPersistentRecord::ModuleBuild(
                    storage.prepare_restore(&self.address, self.etag.as_ref())?,
                ),
                CODE_GENERATION_CACHE_NAMESPACE => PreparedPersistentRecord::CodeGeneration(
                    storage.prepare_restore(&self.address, self.etag.as_ref())?,
                ),
                ASSET_RENDER_CACHE_NAMESPACE => PreparedPersistentRecord::AssetRender(
                    storage.prepare_restore(&self.address, self.etag.as_ref())?,
                ),
                _ => return None,
            }
        };
        #[cfg(test)]
        if let Some(barrier) = &self.barrier {
            barrier.entered.wait();
            barrier.release.wait();
        }

        let restored = match prepared {
            PreparedPersistentRecord::Resolve(prepared) => {
                let record = ResolveRecord::try_from(prepared.decode()?).ok()?;
                CacheEntry::new(CacheItemFamily::Resolve, self.cache_etag.clone(), record)
            }
            PreparedPersistentRecord::ModuleBuild(prepared) => {
                let record = ModuleBuildRecord::try_from(prepared.decode()?).ok()?;
                CacheEntry::new(
                    CacheItemFamily::ModuleBuild,
                    self.cache_etag.clone(),
                    record,
                )
            }
            PreparedPersistentRecord::CodeGeneration(prepared) => {
                let record = prepared.decode()?.into_record_after_codec_validation();
                CacheEntry::new(
                    CacheItemFamily::CodeGeneration,
                    self.cache_etag.clone(),
                    record,
                )
            }
            PreparedPersistentRecord::AssetRender(prepared) => {
                let record = RenderedSource::try_from(prepared.decode()?).ok()?;
                CacheEntry::new(
                    CacheItemFamily::AssetRender,
                    self.cache_etag.clone(),
                    record,
                )
            }
        };
        if let Some(started) = started {
            self.diagnostics.profile(format!(
                "restore items=1; deserialization items=1 duration_us={}",
                started.elapsed().as_micros()
            ));
        }
        Some(restored)
    }

    pub(super) fn reads_from(&self, other: &Self) -> bool {
        self.reader_generation == other.reader_generation
            && Arc::ptr_eq(&self.storage, &other.storage)
    }
}

#[derive(Debug)]
pub(super) struct PersistentTouch {
    storage: Arc<Mutex<TurboPersistenceStorage>>,
    address: PackFileAddress,
    etag: Option<PackFileETag>,
    stamp: AccessStamp,
}

impl PersistentTouch {
    pub(super) fn apply(&self) -> bool {
        self.storage
            .lock()
            .expect("Persistent Cache storage mutex should not be poisoned")
            .touch(&self.address, self.etag.as_ref(), self.stamp)
    }
}

#[derive(Debug)]
pub(super) struct PackFileCacheLayer {
    root: Option<PathBuf>,
    serializer: Serializer,
    storage: Option<Arc<Mutex<TurboPersistenceStorage>>>,
    reader_generation: u64,
    compression: PackFileCompression,
    read_only: bool,
    allow_collecting_memory: bool,
    publication_base: PublicationBase,
    active: bool,
    pending: FxHashMap<CacheAddress, CacheEntry>,
    diagnostics: Arc<CacheDiagnostics>,
    _writer_marker: Option<CacheWriterMarker>,
    #[cfg(test)]
    restore_barrier: Option<RestoreBarrier>,
}

#[derive(Debug)]
struct CacheWriterMarker {
    path: PathBuf,
    contents: String,
}

static NEXT_WRITER_TOKEN: AtomicU64 = AtomicU64::new(1);
static PROCESS_START_ID: OnceLock<String> = OnceLock::new();

impl CacheWriterMarker {
    fn acquire(root: &Path, diagnostics: &CacheDiagnostics) -> Option<Self> {
        if let Err(error) = fs::create_dir_all(root) {
            diagnostics.warn(format!(
                "single-writer diagnostic unavailable at {}: {error}; continuing best-effort",
                root.display()
            ));
            return None;
        }
        let path = root.join(".unpack-writer");
        let pid = std::process::id();
        let start = current_process_start_id();
        let token = NEXT_WRITER_TOKEN.fetch_add(1, Ordering::Relaxed);
        let contents = format!("UNPACK-WRITER-1\npid={pid}\nstart={start}\ntoken={token}\n");
        for _ in 0..2 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    use std::io::Write;
                    if file.write_all(contents.as_bytes()).is_ok() {
                        return Some(Self { path, contents });
                    }
                    let _ = fs::remove_file(&path);
                    return None;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    let existing = fs::read_to_string(&path).unwrap_or_default();
                    if writer_marker_is_live(&existing) {
                        diagnostics.warn(format!(
                            "detected another live writer for Persistent Cache location {}; continuing best-effort without a cross-process lock or merge protocol; contract=trusted-local,linux-supported,single-writer",
                            root.display()
                        ));
                        return None;
                    }
                    let _ = fs::remove_file(&path);
                }
                Err(error) => {
                    diagnostics.warn(format!(
                        "single-writer diagnostic unavailable at {}: {error}; continuing best-effort",
                        root.display()
                    ));
                    return None;
                }
            }
        }
        None
    }
}

impl Drop for CacheWriterMarker {
    fn drop(&mut self) {
        if fs::read_to_string(&self.path).ok().as_deref() == Some(self.contents.as_str()) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn current_process_start_id() -> &'static str {
    PROCESS_START_ID.get_or_init(|| {
        linux_process_start_id(std::process::id()).unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .to_string()
        })
    })
}

fn linux_process_start_id(pid: u32) -> Option<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields = stat
        .rsplit_once(") ")?
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    fields.get(19).map(|value| (*value).to_string())
}

fn writer_marker_is_live(contents: &str) -> bool {
    let value = |prefix: &str| contents.lines().find_map(|line| line.strip_prefix(prefix));
    let Some(pid) = value("pid=").and_then(|pid| pid.parse::<u32>().ok()) else {
        return false;
    };
    let Some(start) = value("start=") else {
        return false;
    };
    if pid == std::process::id() {
        return start == current_process_start_id();
    }
    linux_process_start_id(pid).is_some_and(|candidate| candidate == start)
        || !cfg!(target_os = "linux")
}

impl PackFileCacheLayer {
    pub(super) fn open(options: &CacheOptions, diagnostics: Arc<CacheDiagnostics>) -> Self {
        let serializer = persistent_serializer();
        let root = options.cache_location.clone();
        let compression = options.compression.into();
        let storage = root.as_ref().and_then(|root| {
            match TurboPersistenceStorage::open(
                root,
                serializer.clone(),
                options.readonly,
                options.allow_collecting_memory,
            ) {
                Ok(storage) => {
                    if let Some(reason) = storage.recovery_warning() {
                        diagnostics.warn(format!(
                            "could not restore turbo-persistence at {}: {reason}; treating Persistent Cache as cold and rebuilding it on the next publication",
                            root.display()
                        ));
                    }
                    Some(Arc::new(Mutex::new(storage)))
                }
                Err(error) => {
                    diagnostics.warn(format!(
                        "could not open turbo-persistence at {}: {error}; treating Persistent Cache as cold",
                        root.display()
                    ));
                    None
                }
            }
        });
        let has_standalone_guard = storage.as_ref().is_some_and(|storage| {
            storage
                .lock()
                .expect("Persistent Cache storage mutex should not be poisoned")
                .guard()
                .is_some_and(|guard| {
                    guard.build_dependencies.entries.is_empty()
                        && guard.resolve_build_dependencies.entries.is_empty()
                })
        });
        let publication_base = if has_standalone_guard {
            PublicationBase::PreserveEntries {
                expected_revision: storage
                    .as_ref()
                    .expect("standalone Persistent Cache storage should be open")
                    .lock()
                    .expect("Persistent Cache storage mutex should not be poisoned")
                    .revision(),
            }
        } else {
            PublicationBase::ReplaceAll
        };
        let writer_marker = if options.readonly {
            None
        } else {
            root.as_deref()
                .and_then(|root| CacheWriterMarker::acquire(root, &diagnostics))
        };
        Self {
            root,
            serializer,
            storage,
            reader_generation: 0,
            compression,
            read_only: options.readonly,
            allow_collecting_memory: options.allow_collecting_memory,
            publication_base,
            // Compiler preparation validates non-empty Build Dependency guards before
            // work starts.  Standalone cache users have no such guard, and may safely
            // restore an empty-guard Persistent Cache container directly.
            active: true,
            pending: FxHashMap::default(),
            diagnostics,
            _writer_marker: writer_marker,
            #[cfg(test)]
            restore_barrier: None,
        }
    }

    pub(super) fn publish(
        &mut self,
        guard: PackFileGuardDto,
        stamp: AccessStamp,
        max_age: Duration,
    ) -> io::Result<()> {
        let Some(root) = self.root.clone() else {
            self.pending.clear();
            return Ok(());
        };
        if self.storage.is_none() {
            self.storage = Some(Arc::new(Mutex::new(TurboPersistenceStorage::open(
                &root,
                self.serializer.clone(),
                self.read_only,
                self.allow_collecting_memory,
            )?)));
        }
        let storage = self
            .storage
            .as_ref()
            .expect("Persistent Cache storage should be available for publication");
        let before_entries = storage
            .lock()
            .expect("Persistent Cache storage mutex should not be poisoned")
            .entry_count();
        let queued_items = self.pending.len();
        let serialization_started = Instant::now();
        let mut batch = TurboPersistenceWriteBatch::new();
        storage
            .lock()
            .expect("Persistent Cache storage mutex should not be poisoned")
            .copy_access_updates_to(&mut batch);
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
                    batch.insert(&self.serializer, pack_address, pack_etag, dto)?;
                }
                CacheItemFamily::ModuleBuild => {
                    let record = entry.value::<ModuleBuildRecord>().ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Module Build Cache Item contains an unexpected value",
                        )
                    })?;
                    let dto = ModuleBuildRecordDto::try_from(record.as_ref())?;
                    batch.insert(&self.serializer, pack_address, pack_etag, dto)?;
                }
                CacheItemFamily::CodeGeneration => {
                    let record = entry.value::<CodeGenerationRecord>().ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Code Generation Cache Item contains an unexpected value",
                        )
                    })?;
                    let dto = CodeGenerationRecordDto::from(record.as_ref());
                    batch.insert(&self.serializer, pack_address, pack_etag, dto)?;
                }
                CacheItemFamily::AssetRender => {
                    let record = entry.value::<RenderedSource>().ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Asset Render Cache Item contains an unexpected value",
                        )
                    })?;
                    let dto = AssetRenderRecordDto::from(record.as_ref());
                    batch.insert(&self.serializer, pack_address, pack_etag, dto)?;
                }
            }
        }
        self.diagnostics.profile(format!(
            "serialization items={queued_items} duration_us={}",
            serialization_started.elapsed().as_micros()
        ));
        let store_started = Instant::now();
        let mut storage = storage
            .lock()
            .expect("Persistent Cache storage mutex should not be poisoned");
        let publication = storage.publish(
            guard,
            self.publication_base,
            batch,
            PackFilePublicationOptions::new(
                PackFileRetention::new(stamp, max_age),
                self.compression,
            ),
        )?;
        self.pending.clear();
        let after_entries = storage.entry_count();
        self.diagnostics.profile(format!(
            "store items={queued_items} duration_us={}; garbage collection removed_items={}; turbo-persistence transactions={} compaction={}",
            store_started.elapsed().as_micros(),
            before_entries.saturating_sub(after_entries),
            publication.transaction_count,
            publication.compaction
        ));
        if let Some(error) = publication.compaction_error {
            self.diagnostics.warn(format!(
                "turbo-persistence compaction failed after the cache publication committed: {error}"
            ));
        }
        self.publication_base = PublicationBase::PreserveEntries {
            expected_revision: storage.revision(),
        };
        drop(storage);
        self.reader_generation = self.reader_generation.wrapping_add(1);
        self.active = true;
        Ok(())
    }

    pub(super) fn prepare_persistent(
        &mut self,
        preparation: PersistentCachePreparation<'_>,
    ) -> bool {
        let PersistentCachePreparation {
            guard,
            build_inputs,
            resolved_build_inputs,
            automatic_build_inputs,
            file_system_info,
            build_dependency_snapshot_strategy,
            resolve_build_dependency_snapshot_strategy,
        } = preparation;
        let build_strategy = if !build_inputs.is_empty()
            && build_inputs
                .iter()
                .all(|path| automatic_build_inputs.contains(path))
        {
            SnapshotStrategy::timestamp()
        } else {
            build_dependency_snapshot_strategy
        };
        let active = self.storage.as_ref().is_some_and(|storage| {
            storage
                .lock()
                .expect("Persistent Cache storage mutex should not be poisoned")
                .guard()
                .is_some_and(|candidate| {
                    let build_dependencies =
                        Snapshot::try_from(candidate.build_dependencies.clone()).ok();
                    let resolve_build_dependencies =
                        Snapshot::try_from(candidate.resolve_build_dependencies.clone()).ok();
                    candidate.version == guard.version
                        && build_dependencies.is_some_and(|snapshot| {
                            snapshot.has_exact_paths(build_inputs.iter().cloned())
                                && file_system_info
                                    .is_snapshot_valid_sync(&snapshot, build_strategy)
                        })
                        && resolve_build_dependencies.is_some_and(|snapshot| {
                            file_system_info.is_snapshot_valid_sync(
                                &snapshot,
                                resolve_build_dependency_snapshot_strategy,
                            ) || snapshot.has_valid_paths_sync(
                                resolved_build_inputs.iter().cloned(),
                                resolve_build_dependency_snapshot_strategy,
                                file_system_info,
                            )
                        })
                })
        });
        self.reader_generation = self.reader_generation.wrapping_add(1);
        self.active = active;
        self.publication_base = if active {
            PublicationBase::PreserveEntries {
                expected_revision: self
                    .storage
                    .as_ref()
                    .expect("active Persistent Cache storage should be open")
                    .lock()
                    .expect("Persistent Cache storage mutex should not be poisoned")
                    .revision(),
            }
        } else {
            PublicationBase::ReplaceAll
        };
        self.storage
            .as_ref()
            .and_then(|storage| {
                storage
                    .lock()
                    .expect("Persistent Cache storage mutex should not be poisoned")
                    .guard()
                    .cloned()
            })
            .as_ref()
            != Some(guard)
    }

    pub(super) fn plan_touch(
        &self,
        address: &CacheAddress,
        etag: Option<&CacheETag>,
        stamp: AccessStamp,
    ) -> Option<PersistentTouch> {
        if !self.active {
            return None;
        }
        self.storage.as_ref().map(|storage| PersistentTouch {
            storage: Arc::clone(storage),
            address: address.to_pack_file_address(),
            etag: etag.map(CacheETag::to_pack_file_etag),
            stamp,
        })
    }

    pub(super) fn has_publication(&self) -> bool {
        self.active
    }

    #[cfg(test)]
    pub(super) fn install_restore_barrier(&mut self, barrier: RestoreBarrier) {
        self.restore_barrier = Some(barrier);
    }
}

impl CacheLayer for PackFileCacheLayer {
    fn lookup(&mut self, address: &CacheAddress, etag: Option<&CacheETag>) -> CacheLayerLookup {
        if let Some(entry) = self
            .pending
            .get(address)
            .filter(|entry| entry.etag.as_ref() == etag)
        {
            return CacheLayerLookup::Hit(entry.clone());
        }
        if !self.active {
            return CacheLayerLookup::Miss;
        }
        let Some(storage) = self.storage.as_ref() else {
            return CacheLayerLookup::Miss;
        };
        CacheLayerLookup::Deferred(PersistentRestore {
            storage: Arc::clone(storage),
            reader_generation: self.reader_generation,
            address: address.to_pack_file_address(),
            etag: etag.map(CacheETag::to_pack_file_etag),
            cache_etag: etag.cloned(),
            namespace: address.namespace,
            diagnostics: Arc::clone(&self.diagnostics),
            #[cfg(test)]
            barrier: self.restore_barrier.take(),
        })
    }

    fn store(&mut self, address: CacheAddress, entry: CacheEntry) {
        self.diagnostics.profile("store queued_items=1");
        self.pending.insert(address, entry);
    }

    fn clear(&mut self) {
        self.pending.clear();
        self.active = false;
        self.reader_generation = self.reader_generation.wrapping_add(1);
        self.publication_base = PublicationBase::ReplaceAll;
    }
}

pub(super) fn persistent_serializer() -> Serializer {
    Serializer::new()
        .with_codec::<ResolveRecordDto, _>(ResolveRecordCodec::current())
        .with_codec::<ModuleBuildRecordDto, _>(ModuleBuildRecordCodec::current())
        .with_codec::<CodeGenerationRecordDto, _>(CodeGenerationRecordCodec::current())
        .with_codec::<AssetRenderRecordDto, _>(AssetRenderRecordCodec::current())
}
