//! `turbo-persistence` backed storage for the webpack-shaped Persistent Cache layer.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs, io,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use brotli::{CompressorWriter, Decompressor};
use flate2::{Compression as GzipLevel, read::GzDecoder, write::GzEncoder};
use turbo_persistence::{
    ArcBytes, CompactConfig, DbConfig, FamilyConfig, FamilyKind, SerialScheduler, TurboPersistence,
    ValueBuffer, read_current_version,
};

use crate::serialization::{
    MAX_SERIALIZED_ITEM_BYTES, SerializableItem, Serializer, StableCodecId, StableTypeId,
};

use super::pack_file::{
    AccessStamp, PackFileAddress, PackFileCompression, PackFileETag, PackFileGuardDto,
    PackFilePublicationOptions, PublicationBase, decode_pack_file_guard, encode_pack_file_guard,
};

const DATABASE_DIRECTORY: &str = "turbo-persistence";
const FAMILY: usize = 0;
const MANIFEST_KEY: &[u8] = b"\0unpack-persistent-cache-manifest-v1";
const ENTRY_KEY_PREFIX: u8 = 1;
const MANIFEST_MAGIC: &[u8] = b"UNPACK-TURBO-MANIFEST\0";
const RECORD_MAGIC: &[u8] = b"UNPACK-TURBO-RECORD\0";
const MAX_MANIFEST_BYTES: usize = 16 * 1024 * 1024;
const MAX_ENCODED_RECORD_BYTES: usize = 40 * 1024 * 1024;
const MAX_FIELD_BYTES: usize = 1024 * 1024;
const MAX_MANIFEST_ENTRIES: usize = 100_000;
const BROTLI_BUFFER_BYTES: usize = 16 * 1024;
const BROTLI_QUALITY: u32 = 5;
const BROTLI_WINDOW_BITS: u32 = 22;
const GZIP_LEVEL: u32 = 6;

type Database = TurboPersistence<SerialScheduler, 1>;

#[derive(Clone, Debug, Default)]
struct Manifest {
    revision: u64,
    guard: Option<PackFileGuardDto>,
    entries: BTreeMap<PackFileAddress, EntryMetadata>,
}

#[derive(Clone, Debug)]
struct EntryMetadata {
    etag: Option<PackFileETag>,
    type_id: StableTypeId,
    codec_id: StableCodecId,
    last_access: AccessStamp,
    last_used_revision: u64,
    unused_since_revision: Option<u64>,
}

pub(crate) struct TurboPersistenceStorage {
    root: PathBuf,
    database_path: PathBuf,
    serializer: Serializer,
    database: Option<Database>,
    manifest: Manifest,
    access_updates: BTreeMap<PackFileAddress, AccessStamp>,
    read_only: bool,
    allow_collecting_memory: bool,
    rebuild_before_write: bool,
    recovery_warning: Option<String>,
}

#[derive(Debug)]
pub(super) struct TurboPersistencePublication {
    pub(super) transaction_count: u8,
    pub(super) compaction: &'static str,
    pub(super) compaction_error: Option<String>,
}

impl fmt::Debug for TurboPersistenceStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TurboPersistenceStorage")
            .field("root", &self.root)
            .field("database_path", &self.database_path)
            .field("revision", &self.manifest.revision)
            .field("entries", &self.manifest.entries.len())
            .field("read_only", &self.read_only)
            .finish_non_exhaustive()
    }
}

impl TurboPersistenceStorage {
    pub(super) fn open(
        root: impl AsRef<Path>,
        serializer: Serializer,
        read_only: bool,
        allow_collecting_memory: bool,
    ) -> io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        let database_path = Self::database_directory(&root);
        let mut database = None;
        let mut manifest = Manifest::default();
        let mut rebuild_before_write = false;
        let mut recovery_warning = None;
        match read_current_version(&database_path) {
            Ok(Some(version)) => {
                let opened = if read_only {
                    Database::open_read_only_with_config(database_path.clone(), database_config())
                } else {
                    Database::open_with_config(database_path.clone(), database_config())
                };
                match opened {
                    Ok(opened) => match load_manifest(&opened) {
                        Ok(Some(loaded)) => {
                            manifest = loaded;
                            database = Some(opened);
                        }
                        Ok(None) if version.max_sequence_number == 0 => {
                            database = Some(opened);
                        }
                        result => {
                            let reason = match result {
                                Ok(None) => {
                                    "the committed database has no Unpack manifest".to_string()
                                }
                                Err(error) => error.to_string(),
                                Ok(Some(_)) => unreachable!(),
                            };
                            if read_only {
                                return Err(io::Error::new(io::ErrorKind::InvalidData, reason));
                            }
                            let _ = opened.shutdown();
                            rebuild_before_write = true;
                            recovery_warning = Some(reason);
                        }
                    },
                    Err(error) if read_only => return Err(io_other(error)),
                    Err(error) => {
                        rebuild_before_write = true;
                        recovery_warning = Some(error.to_string());
                    }
                }
            }
            Ok(None) => match database_path.try_exists() {
                Ok(false) => {}
                Ok(true) => {
                    let reason = "the turbo-persistence directory has no CURRENT file".to_string();
                    if read_only {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, reason));
                    }
                    rebuild_before_write = true;
                    recovery_warning = Some(reason);
                }
                Err(error) if read_only => return Err(error),
                Err(error) => {
                    rebuild_before_write = true;
                    recovery_warning = Some(error.to_string());
                }
            },
            Err(error) if read_only => return Err(io_other(error)),
            Err(error) => {
                rebuild_before_write = true;
                recovery_warning = Some(error.to_string());
            }
        }
        Ok(Self {
            root,
            database_path,
            serializer,
            database,
            manifest,
            access_updates: BTreeMap::new(),
            read_only,
            allow_collecting_memory,
            rebuild_before_write,
            recovery_warning,
        })
    }

    pub(super) fn recovery_warning(&self) -> Option<&str> {
        self.recovery_warning.as_deref()
    }

    pub(crate) fn database_directory(root: impl AsRef<Path>) -> PathBuf {
        root.as_ref().join(DATABASE_DIRECTORY)
    }

    #[cfg(test)]
    pub(crate) fn current_path(root: impl AsRef<Path>) -> PathBuf {
        Self::database_directory(root).join("CURRENT")
    }

    pub(super) fn entry_count(&self) -> usize {
        self.manifest.entries.len()
    }

    pub(super) fn revision(&self) -> u64 {
        self.manifest.revision
    }

    pub(super) fn guard(&self) -> Option<&PackFileGuardDto> {
        self.manifest.guard.as_ref()
    }

    pub(super) fn touch(
        &mut self,
        address: &PackFileAddress,
        etag: Option<&PackFileETag>,
        stamp: AccessStamp,
    ) -> bool {
        let Some(entry) = self.manifest.entries.get(address) else {
            return false;
        };
        if entry.etag.as_ref() != etag {
            return false;
        }
        let stamp = stamp.max(entry.last_access);
        match self.access_updates.entry(address.clone()) {
            std::collections::btree_map::Entry::Vacant(vacant) => {
                vacant.insert(stamp);
                true
            }
            std::collections::btree_map::Entry::Occupied(mut occupied)
                if stamp > *occupied.get() =>
            {
                occupied.insert(stamp);
                true
            }
            std::collections::btree_map::Entry::Occupied(_) => false,
        }
    }

    pub(super) fn copy_access_updates_to(&self, batch: &mut TurboPersistenceWriteBatch) {
        for (address, stamp) in &self.access_updates {
            batch.record_access(address.clone(), *stamp);
        }
    }

    pub(super) fn prepare_restore<T: SerializableItem>(
        &self,
        address: &PackFileAddress,
        etag: Option<&PackFileETag>,
    ) -> Option<TurboPersistenceRestore<T>> {
        let entry = self.manifest.entries.get(address)?;
        if entry.etag.as_ref() != etag || entry.type_id != T::TYPE_ID {
            return None;
        }
        if !self.serializer.matches_codec(entry.type_id, entry.codec_id) {
            return None;
        }
        let database = self.database.as_ref()?;
        let key = entry_key(address).ok()?;
        let query = key.as_slice();
        let bytes = database.get(FAMILY, &query).ok()??;
        if !record_matches(&bytes, entry.type_id, entry.codec_id) {
            return None;
        }
        if self.allow_collecting_memory {
            database.clear_block_caches();
        }
        Some(TurboPersistenceRestore {
            serializer: self.serializer.clone(),
            type_id: entry.type_id,
            codec_id: entry.codec_id,
            bytes,
            marker: std::marker::PhantomData,
        })
    }

    pub(super) fn publish(
        &mut self,
        guard: PackFileGuardDto,
        base: PublicationBase,
        batch: TurboPersistenceWriteBatch,
        options: PackFilePublicationOptions,
    ) -> io::Result<TurboPersistencePublication> {
        if self.read_only {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "cannot publish a read-only Persistent Cache",
            ));
        }
        let previous_entries = self.manifest.entries.clone();
        let mut entries = match base {
            PublicationBase::PreserveEntries { expected_revision }
                if expected_revision == self.manifest.revision =>
            {
                previous_entries.clone()
            }
            PublicationBase::PreserveEntries { .. } => {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "turbo-persistence publication base revision changed",
                ));
            }
            PublicationBase::ReplaceAll => BTreeMap::new(),
        };
        let revision = self.manifest.revision.checked_add(1).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Persistent Cache revision overflow",
            )
        })?;
        let TurboPersistenceWriteBatch { items, accesses } = batch;
        let pending_addresses = items.keys().cloned().collect::<BTreeSet<_>>();

        for (address, entry) in &mut entries {
            if pending_addresses.contains(address) {
                continue;
            }
            if let Some(access) = accesses.get(address) {
                entry.last_access = entry.last_access.max(*access);
                entry.last_used_revision = revision;
                entry.unused_since_revision = None;
            } else if entry.unused_since_revision.is_none() {
                entry.unused_since_revision = Some(revision);
            }
        }
        entries.retain(|address, entry| {
            pending_addresses.contains(address)
                || !entry_is_expired(
                    entry,
                    revision,
                    options.retention.max_age,
                    options.retention.now,
                )
        });

        let mut encoded_items = BTreeMap::new();
        for (address, item) in items {
            let value = encode_record(&item, options.compression)?;
            entries.insert(
                address.clone(),
                EntryMetadata {
                    etag: item.etag,
                    type_id: item.type_id,
                    codec_id: item.codec_id,
                    last_access: options.retention.now,
                    last_used_revision: revision,
                    unused_since_revision: None,
                },
            );
            encoded_items.insert(address, value);
        }

        let removed = previous_entries
            .keys()
            .filter(|address| !entries.contains_key(*address))
            .cloned()
            .collect::<Vec<_>>();
        let manifest = Manifest {
            revision,
            guard: Some(guard),
            entries,
        };
        let manifest_bytes = encode_manifest(&manifest)?;

        self.ensure_database()?;
        {
            let database = self
                .database
                .as_ref()
                .expect("writable turbo-persistence database should be open");
            let write_batch = database.write_batch::<Vec<u8>>().map_err(io_other)?;
            for address in removed {
                write_batch
                    .delete(FAMILY as u32, entry_key(&address)?)
                    .map_err(io_other)?;
            }
            for (address, value) in encoded_items {
                write_batch
                    .put(
                        FAMILY as u32,
                        entry_key(&address)?,
                        ValueBuffer::from(value),
                    )
                    .map_err(io_other)?;
            }
            write_batch
                .put(
                    FAMILY as u32,
                    MANIFEST_KEY.to_vec(),
                    ValueBuffer::from(manifest_bytes),
                )
                .map_err(io_other)?;
            force_test_process_termination();
            database.commit_write_batch(write_batch).map_err(io_other)?;
        }

        self.manifest = manifest;
        self.access_updates.clear();
        let database = self
            .database
            .as_ref()
            .expect("committed turbo-persistence database should remain open");
        let (transaction_count, compaction, compaction_error) =
            match database.compact(&CompactConfig::default()) {
                Ok(Some(_)) => (2, "performed", None),
                Ok(None) => (1, "skipped", None),
                Err(error) => (1, "failed", Some(error.to_string())),
            };
        if self.allow_collecting_memory {
            database.clear_cache();
        }
        Ok(TurboPersistencePublication {
            transaction_count,
            compaction,
            compaction_error,
        })
    }

    fn ensure_database(&mut self) -> io::Result<()> {
        if self.rebuild_before_write {
            remove_database_path(&self.database_path)?;
            self.rebuild_before_write = false;
        }
        if self.database.is_none() {
            self.database = Some(
                Database::open_with_config(self.database_path.clone(), database_config())
                    .map_err(io_other)?,
            );
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn revision_at(root: impl AsRef<Path>) -> io::Result<u64> {
        Self::open(root, Serializer::new(), true, true).map(|storage| storage.revision())
    }
}

impl Drop for TurboPersistenceStorage {
    fn drop(&mut self) {
        if let Some(database) = &self.database {
            let _ = database.shutdown();
        }
    }
}

#[derive(Debug)]
pub(super) struct TurboPersistenceRestore<T> {
    serializer: Serializer,
    type_id: StableTypeId,
    codec_id: StableCodecId,
    bytes: ArcBytes,
    marker: std::marker::PhantomData<fn() -> T>,
}

impl<T: SerializableItem> TurboPersistenceRestore<T> {
    pub(super) fn decode(self) -> Option<T> {
        let payload = decode_record(&self.bytes, self.type_id, self.codec_id)?;
        self.serializer
            .decode::<T>(self.type_id, self.codec_id, &payload)
    }
}

#[derive(Debug)]
struct PendingItem {
    etag: Option<PackFileETag>,
    type_id: StableTypeId,
    codec_id: StableCodecId,
    payload: Vec<u8>,
}

#[derive(Debug, Default)]
pub(super) struct TurboPersistenceWriteBatch {
    items: BTreeMap<PackFileAddress, PendingItem>,
    accesses: BTreeMap<PackFileAddress, AccessStamp>,
}

impl TurboPersistenceWriteBatch {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn insert<T: SerializableItem>(
        &mut self,
        serializer: &Serializer,
        address: PackFileAddress,
        etag: Option<PackFileETag>,
        value: T,
    ) -> io::Result<()> {
        let (codec_id, payload) = serializer.encode(&value)?;
        self.items.insert(
            address,
            PendingItem {
                etag,
                type_id: T::TYPE_ID,
                codec_id,
                payload,
            },
        );
        Ok(())
    }

    fn record_access(&mut self, address: PackFileAddress, stamp: AccessStamp) {
        self.accesses
            .entry(address)
            .and_modify(|current| *current = (*current).max(stamp))
            .or_insert(stamp);
    }
}

fn database_config() -> DbConfig<1> {
    DbConfig {
        family_configs: [FamilyConfig {
            name: "unpack-persistent-cache",
            kind: FamilyKind::SingleValue,
        }],
    }
}

fn load_manifest(database: &Database) -> io::Result<Option<Manifest>> {
    let key = MANIFEST_KEY;
    let Some(bytes) = database.get(FAMILY, &key).map_err(io_other)? else {
        return Ok(None);
    };
    decode_manifest(&bytes)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid Unpack cache manifest"))
        .map(Some)
}

fn remove_database_path(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn entry_key(address: &PackFileAddress) -> io::Result<Vec<u8>> {
    let mut encoder = Encoder::default();
    encoder.write_u8(ENTRY_KEY_PREFIX);
    encoder.write_bytes(&address.namespace, MAX_FIELD_BYTES)?;
    encoder.write_bytes(&address.identifier, MAX_FIELD_BYTES)?;
    Ok(encoder.finish())
}

fn entry_is_expired(
    entry: &EntryMetadata,
    candidate_revision: u64,
    max_age: Duration,
    now: AccessStamp,
) -> bool {
    entry
        .unused_since_revision
        .is_some_and(|revision| revision < candidate_revision)
        && now
            .unix_millis
            .checked_sub(entry.last_access.unix_millis)
            .map(Duration::from_millis)
            .is_some_and(|age| age > max_age)
}

fn encode_manifest(manifest: &Manifest) -> io::Result<Vec<u8>> {
    if manifest.entries.len() > MAX_MANIFEST_ENTRIES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Persistent Cache manifest has too many entries",
        ));
    }
    let mut encoder = Encoder::default();
    encoder.write_raw(MANIFEST_MAGIC);
    encoder.write_u64(manifest.revision);
    match &manifest.guard {
        Some(guard) => {
            encoder.write_u8(1);
            encoder.write_bytes(&encode_pack_file_guard(guard)?, MAX_MANIFEST_BYTES)?;
        }
        None => encoder.write_u8(0),
    }
    encoder.write_u32(u32::try_from(manifest.entries.len()).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidData, "manifest entry count overflow")
    })?);
    for (address, entry) in &manifest.entries {
        encoder.write_bytes(&address.namespace, MAX_FIELD_BYTES)?;
        encoder.write_bytes(&address.identifier, MAX_FIELD_BYTES)?;
        encoder.write_optional_bytes(entry.etag.as_ref().map(|etag| etag.0.as_slice()))?;
        encoder.write_raw(entry.type_id.as_bytes());
        encoder.write_raw(&entry.codec_id.0);
        encoder.write_u64(entry.last_access.unix_millis);
        encoder.write_u64(entry.last_used_revision);
        encoder.write_optional_u64(entry.unused_since_revision);
    }
    let bytes = encoder.finish();
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Persistent Cache manifest exceeds the configured bound",
        ));
    }
    Ok(bytes)
}

fn decode_manifest(bytes: &[u8]) -> Option<Manifest> {
    if bytes.len() > MAX_MANIFEST_BYTES || !bytes.starts_with(MANIFEST_MAGIC) {
        return None;
    }
    let mut decoder = Decoder::new(&bytes[MANIFEST_MAGIC.len()..]);
    let revision = decoder.read_u64()?;
    let guard = match decoder.read_u8()? {
        0 => None,
        1 => Some(decode_pack_file_guard(
            decoder.read_bytes(MAX_MANIFEST_BYTES)?,
        )?),
        _ => return None,
    };
    let count = usize::try_from(decoder.read_u32()?).ok()?;
    if count > MAX_MANIFEST_ENTRIES {
        return None;
    }
    let mut entries = BTreeMap::new();
    for _ in 0..count {
        let address = PackFileAddress {
            namespace: decoder.read_bytes(MAX_FIELD_BYTES)?.to_vec(),
            identifier: decoder.read_bytes(MAX_FIELD_BYTES)?.to_vec(),
        };
        let etag = decoder
            .read_optional_bytes(MAX_FIELD_BYTES)?
            .map(|bytes| PackFileETag(bytes.to_vec()));
        let type_id = StableTypeId(decoder.read_array()?);
        let codec_id = StableCodecId(decoder.read_array()?);
        let last_access = AccessStamp::from_millis(decoder.read_u64()?);
        let last_used_revision = decoder.read_u64()?;
        let unused_since_revision = decoder.read_optional_u64()?;
        if last_used_revision > revision
            || unused_since_revision.is_some_and(|unused_since| {
                unused_since > revision || unused_since < last_used_revision
            })
        {
            return None;
        }
        entries.insert(
            address,
            EntryMetadata {
                etag,
                type_id,
                codec_id,
                last_access,
                last_used_revision,
                unused_since_revision,
            },
        );
    }
    decoder.finish()?;
    Some(Manifest {
        revision,
        guard,
        entries,
    })
}

fn encode_record(item: &PendingItem, compression: PackFileCompression) -> io::Result<Vec<u8>> {
    let encoded = match compression {
        PackFileCompression::None => item.payload.clone(),
        PackFileCompression::Gzip => {
            let mut encoder = GzEncoder::new(Vec::new(), GzipLevel::new(GZIP_LEVEL));
            encoder.write_all(&item.payload)?;
            encoder.finish()?
        }
        PackFileCompression::Brotli => {
            let mut encoded = Vec::new();
            {
                let mut encoder = CompressorWriter::new(
                    &mut encoded,
                    BROTLI_BUFFER_BYTES,
                    BROTLI_QUALITY,
                    BROTLI_WINDOW_BITS,
                );
                encoder.write_all(&item.payload)?;
                encoder.flush()?;
            }
            encoded
        }
    };
    if encoded.len() > MAX_ENCODED_RECORD_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "encoded Persistent Cache record exceeds the configured bound",
        ));
    }
    let mut encoder = Encoder::default();
    encoder.write_raw(RECORD_MAGIC);
    encoder.write_raw(item.type_id.as_bytes());
    encoder.write_raw(&item.codec_id.0);
    encoder.write_u8(match compression {
        PackFileCompression::None => 0,
        PackFileCompression::Gzip => 1,
        PackFileCompression::Brotli => 2,
    });
    encoder.write_u64(
        u64::try_from(item.payload.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "record size overflow"))?,
    );
    encoder.write_bytes(&encoded, MAX_ENCODED_RECORD_BYTES)?;
    Ok(encoder.finish())
}

fn record_matches(bytes: &[u8], type_id: StableTypeId, codec_id: StableCodecId) -> bool {
    record_parts(bytes).is_some_and(|parts| parts.type_id == type_id && parts.codec_id == codec_id)
}

fn decode_record(
    bytes: &[u8],
    expected_type_id: StableTypeId,
    expected_codec_id: StableCodecId,
) -> Option<Vec<u8>> {
    let parts = record_parts(bytes)?;
    if parts.type_id != expected_type_id || parts.codec_id != expected_codec_id {
        return None;
    }
    let expected_size = usize::try_from(parts.uncompressed_size).ok()?;
    if expected_size > MAX_SERIALIZED_ITEM_BYTES {
        return None;
    }
    let payload = match parts.compression {
        PackFileCompression::None => {
            (parts.encoded.len() == expected_size).then(|| parts.encoded.to_vec())?
        }
        PackFileCompression::Gzip => {
            read_decompressed(GzDecoder::new(parts.encoded), expected_size)?
        }
        PackFileCompression::Brotli => read_decompressed(
            Decompressor::new(parts.encoded, BROTLI_BUFFER_BYTES),
            expected_size,
        )?,
    };
    Some(payload)
}

struct RecordParts<'a> {
    type_id: StableTypeId,
    codec_id: StableCodecId,
    compression: PackFileCompression,
    uncompressed_size: u64,
    encoded: &'a [u8],
}

fn record_parts(bytes: &[u8]) -> Option<RecordParts<'_>> {
    if !bytes.starts_with(RECORD_MAGIC) {
        return None;
    }
    let mut decoder = Decoder::new(&bytes[RECORD_MAGIC.len()..]);
    let type_id = StableTypeId(decoder.read_array()?);
    let codec_id = StableCodecId(decoder.read_array()?);
    let compression = match decoder.read_u8()? {
        0 => PackFileCompression::None,
        1 => PackFileCompression::Gzip,
        2 => PackFileCompression::Brotli,
        _ => return None,
    };
    let uncompressed_size = decoder.read_u64()?;
    let encoded = decoder.read_bytes(MAX_ENCODED_RECORD_BYTES)?;
    decoder.finish()?;
    Some(RecordParts {
        type_id,
        codec_id,
        compression,
        uncompressed_size,
        encoded,
    })
}

fn read_decompressed(reader: impl Read, expected_size: usize) -> Option<Vec<u8>> {
    let limit = u64::try_from(expected_size).ok()?.checked_add(1)?;
    let mut payload = Vec::with_capacity(expected_size);
    reader.take(limit).read_to_end(&mut payload).ok()?;
    (payload.len() == expected_size).then_some(payload)
}

fn force_test_process_termination() {
    if !cfg!(debug_assertions) {
        return;
    }
    let requested = std::env::var_os("UNPACK_TEST_PERSISTENT_CACHE_CRASH_AT");
    if matches!(
        requested.as_deref(),
        Some(value) if value == "before-transaction-commit"
    ) {
        std::process::abort();
    }
}

fn io_other(error: impl fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

#[derive(Default)]
struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn write_raw(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    fn write_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn write_u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn write_bytes(&mut self, bytes: &[u8], maximum: usize) -> io::Result<()> {
        if bytes.len() > maximum {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Persistent Cache field exceeds the configured bound",
            ));
        }
        self.write_u64(
            u64::try_from(bytes.len())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "field size overflow"))?,
        );
        self.write_raw(bytes);
        Ok(())
    }

    fn write_optional_bytes(&mut self, bytes: Option<&[u8]>) -> io::Result<()> {
        match bytes {
            Some(bytes) => {
                self.write_u8(1);
                self.write_bytes(bytes, MAX_FIELD_BYTES)
            }
            None => {
                self.write_u8(0);
                Ok(())
            }
        }
    }

    fn write_optional_u64(&mut self, value: Option<u64>) {
        match value {
            Some(value) => {
                self.write_u8(1);
                self.write_u64(value);
            }
            None => self.write_u8(0),
        }
    }
}

struct Decoder<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Decoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn finish(self) -> Option<()> {
        (self.position == self.bytes.len()).then_some(())
    }

    fn read_array<const N: usize>(&mut self) -> Option<[u8; N]> {
        let end = self.position.checked_add(N)?;
        let value = self.bytes.get(self.position..end)?.try_into().ok()?;
        self.position = end;
        Some(value)
    }

    fn read_u8(&mut self) -> Option<u8> {
        Some(self.read_array::<1>()?[0])
    }

    fn read_u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.read_array()?))
    }

    fn read_u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.read_array()?))
    }

    fn read_bytes(&mut self, maximum: usize) -> Option<&'a [u8]> {
        let length = usize::try_from(self.read_u64()?).ok()?;
        if length > maximum {
            return None;
        }
        let end = self.position.checked_add(length)?;
        let value = self.bytes.get(self.position..end)?;
        self.position = end;
        Some(value)
    }

    fn read_optional_bytes(&mut self, maximum: usize) -> Option<Option<&'a [u8]>> {
        match self.read_u8()? {
            0 => Some(None),
            1 => Some(Some(self.read_bytes(maximum)?)),
            _ => None,
        }
    }

    fn read_optional_u64(&mut self) -> Option<Option<u64>> {
        match self.read_u8()? {
            0 => Some(None),
            1 => Some(Some(self.read_u64()?)),
            _ => None,
        }
    }
}
