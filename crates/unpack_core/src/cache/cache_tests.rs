// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/Cache.js

use std::{collections::BTreeSet, fs, io, path::Path, sync::Arc, time::Duration};

use filetime::{FileTime, set_file_mtime};
use tempfile::tempdir;

use super::*;
use crate::cache::pack_file_cache_strategy::persistent_serializer;
use crate::{
    ModuleIdentity,
    cache::pack_file::PackFile,
    cache::{ResolveRecord, ResolveRequest},
    cache_facade::{CacheETag, CacheIdentifier, CacheKey},
    snapshot::FileSystemInfo,
};

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
fn filesystem_max_age_defaults_to_sixty_days_and_exceeds_u32_milliseconds() {
    let options = CacheOptions::filesystem();
    assert_eq!(options.max_age, Duration::from_secs(60 * 24 * 60 * 60));
    assert!(options.max_age.as_millis() > u128::from(u32::MAX));

    let mut overridden = options;
    overridden.max_age = Duration::from_millis(u64::from(u32::MAX) + 1);
    assert_eq!(
        overridden.max_age,
        Duration::from_millis(u64::from(u32::MAX) + 1)
    );
}

#[test]
fn shared_persistent_location_warns_without_locking_the_second_writer() {
    let temp = tempdir().expect("create shared cache location");
    let mut options = CacheOptions::filesystem();
    options.cache_location = Some(temp.path().join("cache"));
    let first = Cache::new(options.clone(), SnapshotOptions::default());
    let second = Cache::new(options, SnapshotOptions::default());

    let warnings = second.take_infrastructure_log_events();
    assert!(warnings.iter().any(|event| {
        event.level == InfrastructureLogLevel::Warn
            && event.name == CACHE_WRITER_LOGGER
            && event.message.contains("another live writer")
            && event
                .message
                .contains("without a cross-process lock or merge protocol")
            && event
                .message
                .contains("trusted-local,linux-supported,single-writer")
    }));
    assert!(first.normal_module_factory().is_enabled());
    assert!(second.normal_module_factory().is_enabled());
}

#[test]
fn cache_facades_scope_identical_identifiers_by_namespace_and_etag() {
    let cache = Cache::new(CacheOptions::memory(), SnapshotOptions::default());
    let code_generation = cache.facade::<TestCacheKey, String>(
        CacheNamespace::new("unpack/code-generation"),
        CacheItemFamily::CodeGeneration,
    );
    let asset_render = cache.facade::<TestCacheKey, String>(
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

    let counters = cache.work_counters();
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
    let cache = Cache::new(CacheOptions::memory(), SnapshotOptions::default());
    let code_generation = cache.facade::<TestCacheKey, String>(
        CacheNamespace::new("unpack/code-generation"),
        CacheItemFamily::CodeGeneration,
    );
    let identifier = TestCacheKey("evicted-identifier");

    code_generation.store(identifier.clone(), None, "generated source".to_string());
    code_generation.evict_memory(&identifier);

    assert!(code_generation.get(&identifier, None).is_none());
    assert_eq!(
        cache
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
    let cache = Cache::new(CacheOptions::filesystem(), SnapshotOptions::default());
    let code_generation = cache.facade::<TestCacheKey, String>(
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
        cache
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
async fn slow_persistent_decode_does_not_block_memory_or_other_persistent_hits()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let resource = temp.path().join("src/dep.js");
    write(&resource, "export const value = 1;")?;
    let file_system_info = FileSystemInfo::new();
    let record = ResolveRecord::new(
        ModuleIdentity::new(resource.clone()),
        resource.clone(),
        BTreeSet::from([resource]),
        BTreeSet::new(),
        BTreeSet::new(),
        &file_system_info,
        SnapshotStrategy::timestamp(),
    )
    .await?;
    let persistent_key = ResolveRequest::new(temp.path(), "./persistent");
    let other_persistent_key = ResolveRequest::new(temp.path(), "./other-persistent");
    let memory_key = ResolveRequest::new(temp.path(), "./memory");
    let mut options = CacheOptions::filesystem();
    options.cache_location = Some(temp.path().join("cache"));

    let first = Cache::new(options.clone(), SnapshotOptions::default());
    first
        .normal_module_factory()
        .store(persistent_key.clone(), None, record.clone());
    first
        .normal_module_factory()
        .store(other_persistent_key.clone(), None, record.clone());
    first.flush_to_filesystem()?;
    drop(first);

    options.readonly = true;
    let clock = Arc::new(ManualCacheClock::at_millis(1_000));
    let second = Cache::new_with_clock(options, SnapshotOptions::default(), Arc::clone(&clock));
    let facade = second.normal_module_factory();
    facade.store(memory_key.clone(), None, record);
    let restore_entered = Arc::new(std::sync::Barrier::new(2));
    let restore_release = Arc::new(std::sync::Barrier::new(2));
    second.install_restore_barrier(restore_entered.clone(), restore_release.clone());

    let restore_facade = facade.clone();
    let restore_thread = std::thread::spawn(move || restore_facade.get(&persistent_key, None));
    restore_entered.wait();

    let memory_facade = facade.clone();
    let (completed_tx, completed_rx) = std::sync::mpsc::channel();
    let memory_thread = std::thread::spawn(move || {
        completed_tx
            .send(memory_facade.get(&memory_key, None).is_some())
            .expect("memory-hit observation should still be received");
    });
    let completed_before_restore_release = completed_rx.recv_timeout(Duration::from_secs(1)).ok();

    let other_persistent_facade = facade.clone();
    let (other_completed_tx, other_completed_rx) = std::sync::mpsc::channel();
    let other_persistent_thread = std::thread::spawn(move || {
        other_completed_tx
            .send(
                other_persistent_facade
                    .get(&other_persistent_key, None)
                    .is_some(),
            )
            .expect("persistent-hit observation should still be received");
    });
    let other_completed_before_restore_release =
        other_completed_rx.recv_timeout(Duration::from_secs(1)).ok();

    restore_release.wait();
    assert!(
        restore_thread
            .join()
            .expect("restore thread should finish")
            .is_some()
    );
    memory_thread
        .join()
        .expect("memory-hit thread should finish");
    other_persistent_thread
        .join()
        .expect("other persistent-hit thread should finish");
    assert_eq!(
        completed_before_restore_release,
        Some(true),
        "persistent deserialization must not hold the global Cache lock"
    );
    assert_eq!(
        other_completed_before_restore_release,
        Some(true),
        "record decoding must not hold the PackFile reader lock"
    );
    assert_eq!(
        clock.calls(),
        0,
        "read-only cache hits do not need access stamps"
    );
    assert_eq!(
        second.work_counters().for_family(CacheItemFamily::Resolve),
        CacheItemWork {
            hits: 3,
            misses: 0,
            stores: 1,
            restores: 2,
            evictions: 0,
        }
    );
    Ok(())
}

#[test]
fn finite_memory_generations_evict_only_entries_left_unused_for_the_limit() {
    let mut options = CacheOptions::memory();
    options.max_memory_generations = Some(1);
    let cache = Cache::new(options, SnapshotOptions::default());
    let code_generation = cache.facade::<TestCacheKey, String>(
        CacheNamespace::new("unpack/code-generation"),
        CacheItemFamily::CodeGeneration,
    );
    let unused = TestCacheKey("unused");
    let kept = TestCacheKey("kept");

    code_generation.store(unused.clone(), None, "unused source".to_string());
    code_generation.store(kept.clone(), None, "kept source".to_string());
    cache.on_compilation_completed();

    assert_eq!(
        code_generation
            .get(&kept, None)
            .as_deref()
            .map(String::as_str),
        Some("kept source")
    );
    cache.on_compilation_completed();

    assert!(code_generation.get(&unused, None).is_none());
    assert_eq!(
        code_generation
            .get(&kept, None)
            .as_deref()
            .map(String::as_str),
        Some("kept source")
    );
    assert_eq!(
        cache
            .work_counters()
            .for_family(CacheItemFamily::CodeGeneration)
            .evictions,
        1
    );
}

#[test]
fn finite_memory_generations_keep_entries_until_the_completed_generation_boundary() {
    let mut options = CacheOptions::memory();
    options.max_memory_generations = Some(2);
    let cache = Cache::new(options, SnapshotOptions::default());
    let code_generation = cache.facade::<TestCacheKey, String>(
        CacheNamespace::new("unpack/code-generation"),
        CacheItemFamily::CodeGeneration,
    );
    let identifier = TestCacheKey("generation-boundary");

    code_generation.store(identifier.clone(), None, "source".to_string());
    cache.on_compilation_completed();
    cache.on_compilation_completed();

    assert_eq!(
        code_generation
            .get(&identifier, None)
            .as_deref()
            .map(String::as_str),
        Some("source")
    );
    cache.on_compilation_completed();
    cache.on_compilation_completed();
    cache.on_compilation_completed();

    assert!(code_generation.get(&identifier, None).is_none());
    assert_eq!(
        cache
            .work_counters()
            .for_family(CacheItemFamily::CodeGeneration)
            .evictions,
        1
    );
}

#[test]
fn etag_mismatch_does_not_refresh_an_entrys_generation() {
    let mut options = CacheOptions::memory();
    options.max_memory_generations = Some(2);
    let cache = Cache::new(options, SnapshotOptions::default());
    let code_generation = cache.facade::<TestCacheKey, String>(
        CacheNamespace::new("unpack/code-generation"),
        CacheItemFamily::CodeGeneration,
    );
    let identifier = TestCacheKey("etag-mismatch");

    code_generation.store(
        identifier.clone(),
        Some(CacheETag::new("expected")),
        "source".to_string(),
    );
    cache.on_compilation_completed();
    assert!(
        code_generation
            .get(&identifier, Some(&CacheETag::new("different")))
            .is_none()
    );
    cache.on_compilation_completed();
    cache.on_compilation_completed();

    assert!(
        code_generation
            .get(&identifier, Some(&CacheETag::new("expected")))
            .is_none()
    );
    assert_eq!(
        cache
            .work_counters()
            .for_family(CacheItemFamily::CodeGeneration)
            .evictions,
        1
    );
}

#[test]
fn unbounded_memory_generations_never_age_entries() {
    let cache = Cache::new(CacheOptions::memory(), SnapshotOptions::default());
    let code_generation = cache.facade::<TestCacheKey, String>(
        CacheNamespace::new("unpack/code-generation"),
        CacheItemFamily::CodeGeneration,
    );
    let identifier = TestCacheKey("unbounded");

    code_generation.store(identifier.clone(), None, "source".to_string());
    for _ in 0..100 {
        cache.on_compilation_completed();
    }

    assert_eq!(
        code_generation
            .get(&identifier, None)
            .as_deref()
            .map(String::as_str),
        Some("source")
    );
    assert_eq!(
        cache
            .work_counters()
            .for_family(CacheItemFamily::CodeGeneration)
            .evictions,
        0
    );
}

#[test]
fn zero_filesystem_memory_generations_keep_no_memory_layer() {
    let mut options = CacheOptions::filesystem();
    options.max_memory_generations = Some(0);
    let cache = Cache::new(options, SnapshotOptions::default());
    let code_generation = cache.facade::<TestCacheKey, String>(
        CacheNamespace::new("unpack/code-generation"),
        CacheItemFamily::CodeGeneration,
    );
    let identifier = TestCacheKey("persistent-only");

    code_generation.store(identifier.clone(), None, "source".to_string());

    assert_eq!(
        cache
            .inner
            .cache
            .lock()
            .expect("build cache data mutex should not be poisoned")
            .entry_count(CacheItemFamily::CodeGeneration),
        0
    );
    assert_eq!(
        code_generation
            .get(&identifier, None)
            .as_deref()
            .map(String::as_str),
        Some("source")
    );
    assert_eq!(
        cache
            .work_counters()
            .for_family(CacheItemFamily::CodeGeneration),
        CacheItemWork {
            hits: 1,
            misses: 0,
            stores: 1,
            restores: 0,
            evictions: 0,
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

#[tokio::test]
async fn memory_hits_refresh_persistent_access_before_max_age_gc()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let resource = temp.path().join("src/dep.js");
    write(&resource, "export const value = 1;")?;
    let file_system_info = FileSystemInfo::new();
    let record = ResolveRecord::new(
        ModuleIdentity::new(resource.clone()),
        resource.clone(),
        BTreeSet::from([resource]),
        BTreeSet::new(),
        BTreeSet::new(),
        &file_system_info,
        SnapshotStrategy::timestamp(),
    )
    .await?;
    let first_key = ResolveRequest::new(temp.path(), "./first");
    let recent_key = ResolveRequest::new(temp.path(), "./recent");
    let clock = Arc::new(ManualCacheClock::at_millis(1_000));
    let mut options = CacheOptions::filesystem();
    options.cache_location = Some(temp.path().join("cache"));
    options.max_age = Duration::from_millis(100);

    let first = Cache::new_with_clock(options.clone(), SnapshotOptions::default(), clock.clone());
    first
        .normal_module_factory()
        .store(first_key.clone(), None, record.clone());
    first.flush_to_filesystem()?;
    assert_eq!(pack_revision(&options), 1);

    clock.set_millis(1_050);
    let second = Cache::new_with_clock(options.clone(), SnapshotOptions::default(), clock.clone());
    let facade = second.normal_module_factory();
    assert!(facade.get(&first_key, None).is_some());
    second.flush_to_filesystem()?;
    assert_eq!(pack_revision(&options), 2);

    clock.set_millis(1_120);
    assert!(facade.get(&first_key, None).is_some());
    second.flush_to_filesystem()?;
    assert_eq!(pack_revision(&options), 3);

    clock.set_millis(1_221);
    facade.store(recent_key.clone(), None, record.clone());
    second.flush_to_filesystem()?;
    assert_eq!(pack_revision(&options), 4);

    clock.set_millis(1_222);
    facade.store(recent_key.clone(), None, record);
    second.flush_to_filesystem()?;
    assert_eq!(pack_revision(&options), 5);

    let third = Cache::new_with_clock(options, SnapshotOptions::default(), clock);
    let facade = third.normal_module_factory();
    assert!(facade.get(&first_key, None).is_none());
    assert!(facade.get(&recent_key, None).is_some());
    Ok(())
}

fn pack_revision(options: &CacheOptions) -> u64 {
    PackFile::open(
        options
            .cache_location
            .as_ref()
            .expect("filesystem cache should have a location"),
        persistent_serializer(),
    )
    .revision()
}

fn write(path: impl AsRef<Path>, source: &str) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, source)
}
