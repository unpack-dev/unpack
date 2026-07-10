use std::{
    collections::{BTreeMap, HashMap},
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    Error, Result,
    pack_file::{ManagedItemStateDto, PathBytes, SnapshotDto, SnapshotEntryDto, TimestampDto},
};
use regex::RegexBuilder;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotOptions {
    pub module: SnapshotStrategy,
    pub resolve: SnapshotStrategy,
    pub build_dependencies: SnapshotStrategy,
    pub resolve_build_dependencies: SnapshotStrategy,
    pub managed_paths: Vec<SnapshotPathPattern>,
    pub immutable_paths: Vec<SnapshotPathPattern>,
    pub unmanaged_paths: Vec<SnapshotPathPattern>,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self {
            module: SnapshotStrategy::timestamp(),
            resolve: SnapshotStrategy::timestamp(),
            build_dependencies: SnapshotStrategy::timestamp_and_hash(),
            resolve_build_dependencies: SnapshotStrategy::timestamp_and_hash(),
            managed_paths: vec![SnapshotPathPattern::NodeModules],
            immutable_paths: Vec::new(),
            unmanaged_paths: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SnapshotStrategy {
    pub timestamp: bool,
    pub hash: bool,
}

impl SnapshotStrategy {
    pub const fn timestamp() -> Self {
        Self {
            timestamp: true,
            hash: false,
        }
    }

    pub const fn hash() -> Self {
        Self {
            timestamp: false,
            hash: true,
        }
    }

    pub const fn timestamp_and_hash() -> Self {
        Self {
            timestamp: true,
            hash: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotPathPattern {
    Path(PathBuf),
    Regex { source: String, flags: String },
    NodeModules,
}

impl SnapshotPathPattern {
    fn matches(&self, path: &Path) -> bool {
        match self {
            Self::Path(pattern) => path.starts_with(pattern),
            Self::Regex { source, flags } => {
                let normalized_path = normalize_path_for_matching(path);
                RegexBuilder::new(source)
                    .case_insensitive(flags == "i")
                    .build()
                    .map(|regex| regex.is_match(&normalized_path))
                    .unwrap_or(false)
            }
            Self::NodeModules => has_component(path, "node_modules"),
        }
    }

    fn managed_boundary(&self, path: &Path) -> Option<ManagedPathBoundary> {
        match self {
            Self::Path(pattern) if path.starts_with(pattern) => {
                Some(ManagedPathBoundary::Path(pattern.clone()))
            }
            Self::Regex { source, flags } => {
                let normalized_path = normalize_path_for_matching(path);
                RegexBuilder::new(source)
                    .case_insensitive(flags == "i")
                    .build()
                    .ok()?
                    .captures(&normalized_path)?
                    .get(1)
                    .map(|capture| ManagedPathBoundary::Path(PathBuf::from(capture.as_str())))
            }
            Self::NodeModules if has_component(path, "node_modules") => {
                Some(ManagedPathBoundary::NodeModules)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FileSystemInfo {
    managed_paths: Vec<SnapshotPathPattern>,
    immutable_paths: Vec<SnapshotPathPattern>,
    unmanaged_paths: Vec<SnapshotPathPattern>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SnapshotCache {
    file_snapshots: Arc<Mutex<HashMap<FileSnapshotCacheKey, FileSnapshot>>>,
    context_timestamp_hashes: Arc<Mutex<HashMap<PathBuf, DirectoryTimestampHash>>>,
    context_content_hashes: Arc<Mutex<HashMap<PathBuf, u64>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FileSnapshotCacheKey {
    path: PathBuf,
    strategy: SnapshotStrategy,
}

impl FileSystemInfo {
    pub(crate) fn new() -> Self {
        Self::from_snapshot_options(&SnapshotOptions::default())
    }

    pub(crate) fn from_snapshot_options(options: &SnapshotOptions) -> Self {
        Self {
            managed_paths: options.managed_paths.clone(),
            immutable_paths: options.immutable_paths.clone(),
            unmanaged_paths: options.unmanaged_paths.clone(),
        }
    }

    pub(crate) async fn create_file_snapshot(
        &self,
        path: &Path,
        source: &str,
        strategy: SnapshotStrategy,
    ) -> Result<Snapshot> {
        Snapshot::create_file(path, source, strategy, self).await
    }

    #[cfg(test)]
    pub(crate) async fn create_resolve_snapshot(
        &self,
        files: impl IntoIterator<Item = PathBuf>,
        contexts: impl IntoIterator<Item = PathBuf>,
        missing: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
    ) -> Result<Snapshot> {
        Snapshot::create_resolve(files, contexts, missing, strategy, self, None).await
    }

    pub(crate) async fn create_resolve_snapshot_with_cache(
        &self,
        files: impl IntoIterator<Item = PathBuf>,
        contexts: impl IntoIterator<Item = PathBuf>,
        missing: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
        cache: &SnapshotCache,
    ) -> Result<Snapshot> {
        Snapshot::create_resolve(files, contexts, missing, strategy, self, Some(cache)).await
    }

    #[cfg(test)]
    pub(crate) async fn create_snapshot(
        &self,
        paths: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
    ) -> Result<Snapshot> {
        Snapshot::create(paths, strategy, self).await
    }

    pub(crate) fn create_snapshot_sync(
        &self,
        paths: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
    ) -> Result<Snapshot> {
        Snapshot::create_sync(paths, strategy, self)
    }

    #[cfg(test)]
    pub(crate) async fn is_snapshot_valid(
        &self,
        snapshot: &Snapshot,
        strategy: SnapshotStrategy,
    ) -> bool {
        snapshot.is_valid(strategy, self, None).await
    }

    pub(crate) async fn is_snapshot_valid_with_cache(
        &self,
        snapshot: &Snapshot,
        strategy: SnapshotStrategy,
        cache: &SnapshotCache,
    ) -> bool {
        snapshot.is_valid(strategy, self, Some(cache)).await
    }

    pub(crate) fn is_snapshot_valid_sync(
        &self,
        snapshot: &Snapshot,
        strategy: SnapshotStrategy,
    ) -> bool {
        snapshot.is_valid_sync(strategy, self)
    }

    pub(crate) fn merge_snapshots<'a>(
        &self,
        snapshots: impl IntoIterator<Item = &'a Snapshot>,
    ) -> Snapshot {
        Snapshot::merge(snapshots)
    }

    fn classify_path(&self, path: &Path) -> SnapshotPathClassification {
        if self
            .unmanaged_paths
            .iter()
            .any(|pattern| pattern.matches(path))
        {
            return SnapshotPathClassification::Unmanaged;
        }
        if self
            .immutable_paths
            .iter()
            .any(|pattern| pattern.matches(path))
        {
            return SnapshotPathClassification::Immutable;
        }
        for pattern in &self.managed_paths {
            if let Some(boundary) = pattern.managed_boundary(path) {
                return SnapshotPathClassification::Managed(boundary);
            }
        }
        SnapshotPathClassification::Unclassified
    }

    fn ordinary_snapshot_applies(&self, path: &Path) -> bool {
        match self.classify_path(path) {
            SnapshotPathClassification::Unmanaged | SnapshotPathClassification::Unclassified => {
                true
            }
            SnapshotPathClassification::Immutable => false,
            SnapshotPathClassification::Managed(boundary) => {
                ManagedPathSnapshot::create(path, &boundary).is_none()
            }
        }
    }
}

impl Default for FileSystemInfo {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SnapshotPathClassification {
    Unmanaged,
    Immutable,
    Managed(ManagedPathBoundary),
    Unclassified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ManagedPathBoundary {
    NodeModules,
    Path(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    entries: Vec<SnapshotEntry>,
}

impl Snapshot {
    pub(crate) fn to_pack_file_dto(&self) -> SnapshotDto {
        SnapshotDto {
            entries: self
                .entries
                .iter()
                .map(|entry| match entry {
                    SnapshotEntry::File(file) => SnapshotEntryDto::File {
                        path: PathBytes::from_path(&file.path),
                        exists: file.snapshot.exists,
                        modified: file.snapshot.modified.map(system_time_to_dto),
                        source_hash: file.snapshot.source_hash,
                    },
                    SnapshotEntry::Context(context) => SnapshotEntryDto::Context {
                        path: PathBytes::from_path(&context.path),
                        exists: context.snapshot.exists,
                        timestamp_hash: context.snapshot.timestamp_hash,
                        content_hash: context.snapshot.content_hash,
                    },
                    SnapshotEntry::MissingExistence { path } => {
                        SnapshotEntryDto::MissingExistence {
                            path: PathBytes::from_path(path),
                        }
                    }
                    SnapshotEntry::ImmutablePath { path } => SnapshotEntryDto::ImmutablePath {
                        path: PathBytes::from_path(path),
                    },
                    SnapshotEntry::ManagedPath(snapshot) => SnapshotEntryDto::ManagedPath {
                        path: PathBytes::from_path(&snapshot.path),
                        root: PathBytes::from_path(&snapshot.root),
                        state: match &snapshot.state {
                            ManagedItemState::NodeModules => ManagedItemStateDto::NodeModules,
                            ManagedItemState::GroupingFolder => ManagedItemStateDto::GroupingFolder,
                            ManagedItemState::Package { name, version } => {
                                ManagedItemStateDto::Package {
                                    name: name.clone(),
                                    version: version.clone(),
                                }
                            }
                        },
                    },
                })
                .collect(),
        }
    }

    pub(crate) fn from_pack_file_dto(dto: SnapshotDto) -> Option<Self> {
        let entries = dto
            .entries
            .into_iter()
            .map(|entry| match entry {
                SnapshotEntryDto::File {
                    path,
                    exists,
                    modified,
                    source_hash,
                } => Some(SnapshotEntry::File(SnapshottedFile {
                    path: path.to_path_buf(),
                    snapshot: FileSnapshot {
                        exists,
                        modified: match modified {
                            Some(value) => Some(system_time_from_dto(value)?),
                            None => None,
                        },
                        source_hash,
                    },
                })),
                SnapshotEntryDto::Context {
                    path,
                    exists,
                    timestamp_hash,
                    content_hash,
                } => Some(SnapshotEntry::Context(SnapshottedContext {
                    path: path.to_path_buf(),
                    snapshot: ContextSnapshot {
                        exists,
                        timestamp_hash,
                        content_hash,
                    },
                })),
                SnapshotEntryDto::MissingExistence { path } => {
                    Some(SnapshotEntry::MissingExistence {
                        path: path.to_path_buf(),
                    })
                }
                SnapshotEntryDto::ImmutablePath { path } => Some(SnapshotEntry::ImmutablePath {
                    path: path.to_path_buf(),
                }),
                SnapshotEntryDto::ManagedPath { path, root, state } => {
                    Some(SnapshotEntry::ManagedPath(ManagedPathSnapshot {
                        path: path.to_path_buf(),
                        root: root.to_path_buf(),
                        state: match state {
                            ManagedItemStateDto::NodeModules => ManagedItemState::NodeModules,
                            ManagedItemStateDto::GroupingFolder => ManagedItemState::GroupingFolder,
                            ManagedItemStateDto::Package { name, version } => {
                                ManagedItemState::Package { name, version }
                            }
                        },
                    }))
                }
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self { entries })
    }

    async fn create_file(
        path: &Path,
        source: &str,
        strategy: SnapshotStrategy,
        file_system_info: &FileSystemInfo,
    ) -> Result<Self> {
        Ok(Self {
            entries: vec![
                SnapshotEntry::create_file(path, Some(source), strategy, file_system_info).await?,
            ],
        })
    }

    async fn create_resolve(
        files: impl IntoIterator<Item = PathBuf>,
        contexts: impl IntoIterator<Item = PathBuf>,
        missing: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
        file_system_info: &FileSystemInfo,
        cache: Option<&SnapshotCache>,
    ) -> Result<Self> {
        let files = normalize_paths(files);
        let contexts = normalize_paths(contexts);
        let missing = normalize_paths(missing);
        let mut entries = Vec::with_capacity(files.len() + contexts.len() + missing.len());

        for path in files {
            entries
                .push(SnapshotEntry::create_file(&path, None, strategy, file_system_info).await?);
        }
        for path in contexts {
            entries.push(
                SnapshotEntry::create_context(&path, strategy, file_system_info, cache).await?,
            );
        }
        for path in missing {
            entries.push(SnapshotEntry::create_missing(&path, file_system_info));
        }

        Ok(Self { entries })
    }

    #[cfg(test)]
    async fn create(
        paths: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
        file_system_info: &FileSystemInfo,
    ) -> Result<Self> {
        let paths = normalize_paths(paths);
        let mut entries = Vec::with_capacity(paths.len());
        for path in paths {
            entries
                .push(SnapshotEntry::create_file(&path, None, strategy, file_system_info).await?);
        }
        Ok(Self { entries })
    }

    fn create_sync(
        paths: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
        file_system_info: &FileSystemInfo,
    ) -> Result<Self> {
        let paths = normalize_paths(paths);
        let mut entries = Vec::with_capacity(paths.len());
        for path in paths {
            entries.push(SnapshotEntry::create_file_sync(
                &path,
                strategy,
                file_system_info,
            )?);
        }
        Ok(Self { entries })
    }

    async fn is_valid(
        &self,
        strategy: SnapshotStrategy,
        file_system_info: &FileSystemInfo,
        cache: Option<&SnapshotCache>,
    ) -> bool {
        for entry in &self.entries {
            if !entry.is_valid(strategy, file_system_info, cache).await {
                return false;
            }
        }
        true
    }

    fn is_valid_sync(&self, strategy: SnapshotStrategy, file_system_info: &FileSystemInfo) -> bool {
        for entry in &self.entries {
            if !entry.is_valid_sync(strategy, file_system_info) {
                return false;
            }
        }
        true
    }

    pub(crate) fn has_exact_paths(&self, paths: impl IntoIterator<Item = PathBuf>) -> bool {
        let paths = normalize_paths(paths);
        self.entries.len() == paths.len()
            && self
                .entries
                .iter()
                .zip(paths.iter())
                .all(|(entry, path)| entry.path() == path)
    }

    fn merge<'a>(snapshots: impl IntoIterator<Item = &'a Snapshot>) -> Self {
        let mut entries = BTreeMap::new();
        for snapshot in snapshots {
            for entry in &snapshot.entries {
                entries.insert(entry.path().to_path_buf(), entry.clone());
            }
        }

        Self {
            entries: entries.into_values().collect(),
        }
    }
}

fn system_time_to_dto(value: SystemTime) -> TimestampDto {
    match value.duration_since(UNIX_EPOCH) {
        Ok(duration) => TimestampDto {
            seconds: i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
            nanoseconds: duration.subsec_nanos(),
        },
        Err(error) => {
            let duration = error.duration();
            if duration.subsec_nanos() == 0 {
                TimestampDto {
                    seconds: -i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
                    nanoseconds: 0,
                }
            } else {
                TimestampDto {
                    seconds: -i64::try_from(duration.as_secs()).unwrap_or(i64::MAX) - 1,
                    nanoseconds: 1_000_000_000 - duration.subsec_nanos(),
                }
            }
        }
    }
}

fn system_time_from_dto(value: TimestampDto) -> Option<SystemTime> {
    if value.nanoseconds >= 1_000_000_000 {
        return None;
    }
    if value.seconds >= 0 {
        UNIX_EPOCH.checked_add(std::time::Duration::new(
            value.seconds as u64,
            value.nanoseconds,
        ))
    } else if value.nanoseconds == 0 {
        UNIX_EPOCH.checked_sub(std::time::Duration::new(value.seconds.unsigned_abs(), 0))
    } else {
        UNIX_EPOCH.checked_sub(std::time::Duration::new(
            value.seconds.unsigned_abs() - 1,
            1_000_000_000 - value.nanoseconds,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum SnapshotEntry {
    File(SnapshottedFile),
    Context(SnapshottedContext),
    MissingExistence { path: PathBuf },
    ImmutablePath { path: PathBuf },
    ManagedPath(ManagedPathSnapshot),
}

impl SnapshotEntry {
    fn path(&self) -> &Path {
        match self {
            Self::File(file) => &file.path,
            Self::Context(context) => &context.path,
            Self::MissingExistence { path } | Self::ImmutablePath { path } => path,
            Self::ManagedPath(snapshot) => &snapshot.path,
        }
    }

    async fn create_file(
        path: &Path,
        source: Option<&str>,
        strategy: SnapshotStrategy,
        file_system_info: &FileSystemInfo,
    ) -> Result<Self> {
        if let Some(entry) = Self::classified_path_entry(path, file_system_info) {
            return Ok(entry);
        }

        let snapshot = match source {
            Some(source) => FileSnapshot::create(path, source, strategy).await?,
            None => FileSnapshot::create_from_path(path, strategy).await?,
        };
        Ok(Self::File(SnapshottedFile {
            path: path.to_path_buf(),
            snapshot,
        }))
    }

    fn create_file_sync(
        path: &Path,
        strategy: SnapshotStrategy,
        file_system_info: &FileSystemInfo,
    ) -> Result<Self> {
        if let Some(entry) = Self::classified_path_entry(path, file_system_info) {
            return Ok(entry);
        }

        Ok(Self::File(SnapshottedFile {
            path: path.to_path_buf(),
            snapshot: FileSnapshot::create_from_file_sync(path, strategy)?,
        }))
    }

    async fn create_context(
        path: &Path,
        strategy: SnapshotStrategy,
        file_system_info: &FileSystemInfo,
        cache: Option<&SnapshotCache>,
    ) -> Result<Self> {
        if let Some(entry) = Self::classified_path_entry(path, file_system_info) {
            return Ok(entry);
        }

        Ok(Self::Context(SnapshottedContext {
            path: path.to_path_buf(),
            snapshot: ContextSnapshot::create(path, strategy, file_system_info, cache).await?,
        }))
    }

    fn create_missing(path: &Path, file_system_info: &FileSystemInfo) -> Self {
        if let Some(entry) = Self::classified_path_entry(path, file_system_info) {
            return entry;
        }

        Self::MissingExistence {
            path: path.to_path_buf(),
        }
    }

    fn classified_path_entry(path: &Path, file_system_info: &FileSystemInfo) -> Option<Self> {
        match file_system_info.classify_path(path) {
            SnapshotPathClassification::Managed(boundary) => {
                ManagedPathSnapshot::create(path, &boundary).map(Self::ManagedPath)
            }
            SnapshotPathClassification::Immutable => Some(Self::ImmutablePath {
                path: path.to_path_buf(),
            }),
            SnapshotPathClassification::Unmanaged | SnapshotPathClassification::Unclassified => {
                None
            }
        }
    }

    async fn is_valid(
        &self,
        strategy: SnapshotStrategy,
        file_system_info: &FileSystemInfo,
        cache: Option<&SnapshotCache>,
    ) -> bool {
        match self {
            Self::File(file) => {
                file_system_info.ordinary_snapshot_applies(&file.path)
                    && file.snapshot.is_valid(&file.path, strategy, cache).await
            }
            Self::Context(context) => {
                file_system_info.ordinary_snapshot_applies(&context.path)
                    && context
                        .snapshot
                        .is_valid(&context.path, strategy, file_system_info, cache)
                        .await
            }
            Self::MissingExistence { path } => {
                file_system_info.ordinary_snapshot_applies(path)
                    && MissingExistenceSnapshot::is_valid(path).await
            }
            Self::ImmutablePath { path } => {
                file_system_info.classify_path(path) == SnapshotPathClassification::Immutable
            }
            Self::ManagedPath(snapshot) => {
                matches!(
                    file_system_info.classify_path(&snapshot.path),
                    SnapshotPathClassification::Managed(_)
                ) && snapshot.is_valid()
            }
        }
    }

    fn is_valid_sync(&self, strategy: SnapshotStrategy, file_system_info: &FileSystemInfo) -> bool {
        match self {
            Self::File(file) => {
                file_system_info.ordinary_snapshot_applies(&file.path)
                    && file.snapshot.is_valid_sync(&file.path, strategy)
            }
            Self::Context(context) => {
                file_system_info.ordinary_snapshot_applies(&context.path)
                    && context.snapshot.is_valid_sync(
                        &context.path,
                        strategy,
                        file_system_info,
                        None,
                    )
            }
            Self::MissingExistence { path } => {
                file_system_info.ordinary_snapshot_applies(path)
                    && MissingExistenceSnapshot::is_valid_sync(path)
            }
            Self::ImmutablePath { path } => {
                file_system_info.classify_path(path) == SnapshotPathClassification::Immutable
            }
            Self::ManagedPath(snapshot) => {
                matches!(
                    file_system_info.classify_path(&snapshot.path),
                    SnapshotPathClassification::Managed(_)
                ) && snapshot.is_valid()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ContextSnapshot {
    exists: bool,
    #[serde(default)]
    timestamp_hash: Option<u64>,
    #[serde(default, alias = "entries_hash")]
    content_hash: Option<u64>,
}

impl ContextSnapshot {
    async fn create(
        path: &Path,
        strategy: SnapshotStrategy,
        file_system_info: &FileSystemInfo,
        cache: Option<&SnapshotCache>,
    ) -> Result<Self> {
        Self::create_sync(path, strategy, file_system_info, cache)
    }

    fn create_sync(
        path: &Path,
        strategy: SnapshotStrategy,
        file_system_info: &FileSystemInfo,
        cache: Option<&SnapshotCache>,
    ) -> Result<Self> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self {
                    exists: false,
                    timestamp_hash: None,
                    content_hash: None,
                });
            }
            Err(error) => return Err(Error::read(path, error)),
        };

        if !metadata.is_dir() {
            return Ok(Self {
                exists: false,
                timestamp_hash: None,
                content_hash: None,
            });
        }

        let timestamp_hash = if strategy.timestamp {
            Some(directory_timestamp_hash(path, file_system_info, cache)?.hash)
        } else {
            None
        };
        let content_hash = strategy
            .hash
            .then(|| directory_content_hash(path, file_system_info, cache))
            .transpose()?;

        Ok(Self {
            exists: true,
            timestamp_hash,
            content_hash,
        })
    }

    async fn is_valid(
        &self,
        path: &Path,
        strategy: SnapshotStrategy,
        file_system_info: &FileSystemInfo,
        cache: Option<&SnapshotCache>,
    ) -> bool {
        self.is_valid_sync(path, strategy, file_system_info, cache)
    }

    fn is_valid_sync(
        &self,
        path: &Path,
        strategy: SnapshotStrategy,
        file_system_info: &FileSystemInfo,
        cache: Option<&SnapshotCache>,
    ) -> bool {
        if !self.exists {
            return matches!(fs::metadata(path), Err(error) if error.kind() == io::ErrorKind::NotFound);
        }

        let Ok(metadata) = fs::metadata(path) else {
            return false;
        };
        if !metadata.is_dir() {
            return false;
        }

        if strategy.timestamp {
            match directory_timestamp_hash(path, file_system_info, cache) {
                Ok(current) if Some(current.hash) == self.timestamp_hash => return true,
                Ok(_) if strategy.hash => {}
                _ => return false,
            }
        }

        if strategy.hash
            && directory_content_hash(path, file_system_info, cache).ok() != self.content_hash
        {
            return false;
        }

        true
    }
}

struct MissingExistenceSnapshot;

impl MissingExistenceSnapshot {
    async fn is_valid(path: &Path) -> bool {
        Self::is_valid_sync(path)
    }

    fn is_valid_sync(path: &Path) -> bool {
        matches!(fs::metadata(path), Err(error) if error.kind() == io::ErrorKind::NotFound)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FileSnapshot {
    exists: bool,
    modified: Option<SystemTime>,
    source_hash: Option<u64>,
}

impl FileSnapshot {
    pub(crate) async fn create(
        path: &Path,
        source: &str,
        strategy: SnapshotStrategy,
    ) -> Result<Self> {
        let modified = if strategy.timestamp {
            let metadata = tokio::fs::metadata(path)
                .await
                .map_err(|error| Error::read(path, error))?;
            Some(
                metadata
                    .modified()
                    .map_err(|error| Error::read(path, error))?,
            )
        } else {
            None
        };
        let source_hash = strategy.hash.then(|| hash_bytes(source.as_bytes()));

        Ok(Self {
            exists: true,
            modified,
            source_hash,
        })
    }

    pub(crate) async fn create_from_path(path: &Path, strategy: SnapshotStrategy) -> Result<Self> {
        let metadata = match tokio::fs::metadata(path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self {
                    exists: false,
                    modified: None,
                    source_hash: None,
                });
            }
            Err(error) => return Err(Error::read(path, error)),
        };
        let modified = if strategy.timestamp {
            Some(
                metadata
                    .modified()
                    .map_err(|error| Error::read(path, error))?,
            )
        } else {
            None
        };
        let source_hash = if strategy.hash {
            let source = tokio::fs::read(path)
                .await
                .map_err(|error| Error::read(path, error))?;
            Some(hash_bytes(&source))
        } else {
            None
        };

        Ok(Self {
            exists: true,
            modified,
            source_hash,
        })
    }

    pub(crate) async fn is_valid(
        &self,
        path: &Path,
        strategy: SnapshotStrategy,
        cache: Option<&SnapshotCache>,
    ) -> bool {
        let current = match cache {
            Some(cache) => Self::current_with_cache(path, strategy, cache).await,
            None => Self::create_from_path(path, strategy).await.ok(),
        };
        current.as_ref() == Some(self)
    }

    async fn current_with_cache(
        path: &Path,
        strategy: SnapshotStrategy,
        cache: &SnapshotCache,
    ) -> Option<Self> {
        let key = FileSnapshotCacheKey {
            path: path.to_path_buf(),
            strategy,
        };
        if let Some(snapshot) = cache
            .file_snapshots
            .lock()
            .expect("snapshot cache mutex should not be poisoned")
            .get(&key)
            .cloned()
        {
            return Some(snapshot);
        }

        let snapshot = Self::create_from_path(path, strategy).await.ok()?;
        cache
            .file_snapshots
            .lock()
            .expect("snapshot cache mutex should not be poisoned")
            .insert(key, snapshot.clone());
        Some(snapshot)
    }

    pub(crate) fn create_from_file_sync(path: &Path, strategy: SnapshotStrategy) -> Result<Self> {
        let source = fs::read(path).map_err(|error| Error::read(path, error))?;
        let modified = if strategy.timestamp {
            let metadata = fs::metadata(path).map_err(|error| Error::read(path, error))?;
            Some(
                metadata
                    .modified()
                    .map_err(|error| Error::read(path, error))?,
            )
        } else {
            None
        };
        let source_hash = strategy.hash.then(|| hash_bytes(&source));

        Ok(Self {
            exists: true,
            modified,
            source_hash,
        })
    }

    pub(crate) fn is_valid_sync(&self, path: &Path, strategy: SnapshotStrategy) -> bool {
        if !self.exists {
            return !strategy.timestamp && !strategy.hash
                || matches!(fs::metadata(path), Err(error) if error.kind() == io::ErrorKind::NotFound);
        }

        if strategy.timestamp {
            let Ok(metadata) = fs::metadata(path) else {
                return false;
            };
            let Ok(modified) = metadata.modified() else {
                return false;
            };
            if Some(modified) != self.modified {
                return false;
            }
        }

        if strategy.hash {
            let Ok(source) = fs::read(path) else {
                return false;
            };
            if Some(hash_bytes(&source)) != self.source_hash {
                return false;
            }
        }

        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ManagedPathSnapshot {
    path: PathBuf,
    root: PathBuf,
    state: ManagedItemState,
}

impl ManagedPathSnapshot {
    fn create(path: &Path, boundary: &ManagedPathBoundary) -> Option<Self> {
        let root = managed_item_root(path, boundary)?;
        let state = ManagedItemState::create(&root)?;
        Some(Self {
            path: path.to_path_buf(),
            state,
            root,
        })
    }

    fn is_valid(&self) -> bool {
        ManagedItemState::create(&self.root).is_some_and(|state| state == self.state)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum ManagedItemState {
    NodeModules,
    GroupingFolder,
    Package { name: String, version: String },
}

impl ManagedItemState {
    fn create(root: &Path) -> Option<Self> {
        if path_file_name(root) == Some("node_modules") {
            return Some(Self::NodeModules);
        }

        if path_file_name(root).is_some_and(|name| name.starts_with('@')) {
            return Some(Self::GroupingFolder);
        }

        let package_json = root.join("package.json");
        let source = fs::read(&package_json).ok()?;
        let json = serde_json::from_slice::<serde_json::Value>(&source).ok()?;
        let name = json.get("name")?.as_str()?.to_string();
        let version = json.get("version")?.as_str()?.to_string();

        Some(Self::Package { name, version })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SnapshottedFile {
    path: PathBuf,
    snapshot: FileSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SnapshottedContext {
    path: PathBuf,
    snapshot: ContextSnapshot,
}

fn normalize_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut paths = paths.into_iter().collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DirectoryTimestampHash {
    hash: u64,
    is_dir: bool,
}

fn directory_timestamp_hash(
    path: &Path,
    file_system_info: &FileSystemInfo,
    cache: Option<&SnapshotCache>,
) -> Result<DirectoryTimestampHash> {
    if let Some(cache) = cache {
        if let Some(hash) = cache
            .context_timestamp_hashes
            .lock()
            .expect("snapshot cache mutex should not be poisoned")
            .get(path)
            .copied()
        {
            return Ok(hash);
        }
    }

    let hash = directory_timestamp_hash_uncached(path, file_system_info, cache)?;
    if let Some(cache) = cache {
        cache
            .context_timestamp_hashes
            .lock()
            .expect("snapshot cache mutex should not be poisoned")
            .insert(path.to_path_buf(), hash);
    }
    Ok(hash)
}

fn directory_timestamp_hash_uncached(
    path: &Path,
    file_system_info: &FileSystemInfo,
    cache: Option<&SnapshotCache>,
) -> Result<DirectoryTimestampHash> {
    let metadata = fs::symlink_metadata(path).map_err(|error| Error::read(path, error))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Ok(DirectoryTimestampHash {
            hash: modified_time_hash(path, &metadata)?,
            is_dir: false,
        });
    }

    let mut entries = fs::read_dir(path)
        .map_err(|error| Error::read(path, error))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| Error::read(path, error))?;
    entries.sort_by_key(|entry| entry.file_name());

    let mut hasher = StableHasher::new();
    for entry in entries {
        let child_path = entry.path();
        if matches!(
            file_system_info.classify_path(&child_path),
            SnapshotPathClassification::Immutable
        ) {
            continue;
        }

        hasher.write_str(&entry.file_name().to_string_lossy());

        if let SnapshotPathClassification::Managed(boundary) =
            file_system_info.classify_path(&child_path)
        {
            if let Some(snapshot) = ManagedPathSnapshot::create(&child_path, &boundary) {
                write_managed_timestamp_hash(&snapshot.state, &mut hasher);
            }
            continue;
        }

        match directory_timestamp_hash(&child_path, file_system_info, cache) {
            Ok(child_hash) => {
                hasher.write_str(if child_hash.is_dir { "d" } else { "f" });
                hasher.write_u64(child_hash.hash);
            }
            Err(_) => hasher.write_str("n"),
        }
    }

    Ok(DirectoryTimestampHash {
        hash: hasher.finish(),
        is_dir: true,
    })
}

fn directory_content_hash(
    path: &Path,
    file_system_info: &FileSystemInfo,
    cache: Option<&SnapshotCache>,
) -> Result<u64> {
    if let Some(cache) = cache {
        if let Some(hash) = cache
            .context_content_hashes
            .lock()
            .expect("snapshot cache mutex should not be poisoned")
            .get(path)
            .copied()
        {
            return Ok(hash);
        }
    }

    let hash = directory_content_hash_uncached(path, file_system_info, cache)?;
    if let Some(cache) = cache {
        cache
            .context_content_hashes
            .lock()
            .expect("snapshot cache mutex should not be poisoned")
            .insert(path.to_path_buf(), hash);
    }
    Ok(hash)
}

fn directory_content_hash_uncached(
    path: &Path,
    file_system_info: &FileSystemInfo,
    cache: Option<&SnapshotCache>,
) -> Result<u64> {
    let metadata = fs::symlink_metadata(path).map_err(|error| Error::read(path, error))?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        let mut entries = fs::read_dir(path)
            .map_err(|error| Error::read(path, error))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| Error::read(path, error))?;
        entries.sort_by_key(|entry| entry.file_name());

        let mut hasher = StableHasher::new();
        for entry in entries {
            let child_path = entry.path();
            if matches!(
                file_system_info.classify_path(&child_path),
                SnapshotPathClassification::Immutable
            ) {
                continue;
            }

            if let SnapshotPathClassification::Managed(boundary) =
                file_system_info.classify_path(&child_path)
            {
                if let Some(snapshot) = ManagedPathSnapshot::create(&child_path, &boundary) {
                    write_managed_content_hash(&snapshot.state, &mut hasher);
                }
                continue;
            }

            hasher.write_u64(directory_content_hash(
                &child_path,
                file_system_info,
                cache,
            )?);
        }
        return Ok(hasher.finish());
    }

    if metadata.file_type().is_symlink() {
        let target = fs::canonicalize(path).map_err(|error| Error::read(path, error))?;
        return Ok(hash_bytes(target.to_string_lossy().as_bytes()));
    }

    if metadata.is_file() {
        let source = fs::read(path).map_err(|error| Error::read(path, error))?;
        return Ok(hash_bytes(&source));
    }

    Ok(hash_bytes(&[]))
}

fn modified_time_hash(path: &Path, metadata: &fs::Metadata) -> Result<u64> {
    let modified = metadata
        .modified()
        .map_err(|error| Error::read(path, error))?;
    Ok(system_time_hash(modified))
}

fn system_time_hash(time: SystemTime) -> u64 {
    let mut hasher = StableHasher::new();
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            hasher.write_str("+");
            hasher.write_u64(duration.as_secs());
            hasher.write_u64(u64::from(duration.subsec_nanos()));
        }
        Err(error) => {
            let duration = error.duration();
            hasher.write_str("-");
            hasher.write_u64(duration.as_secs());
            hasher.write_u64(u64::from(duration.subsec_nanos()));
        }
    }
    hasher.finish()
}

fn write_managed_timestamp_hash(state: &ManagedItemState, hasher: &mut StableHasher) {
    if let ManagedItemState::Package { version, .. } = state {
        hasher.write_str("d");
        hasher.write_str(version);
    }
}

fn write_managed_content_hash(state: &ManagedItemState, hasher: &mut StableHasher) {
    if let ManagedItemState::Package { version, .. } = state {
        hasher.write_str(version);
    }
}

fn managed_item_root(path: &Path, boundary: &ManagedPathBoundary) -> Option<PathBuf> {
    match boundary {
        ManagedPathBoundary::NodeModules => managed_node_modules_item_root(path).flatten(),
        ManagedPathBoundary::Path(boundary) => managed_node_modules_item_root(path)
            .flatten()
            .or_else(|| nearest_package_root_within(path, boundary))
            .or_else(|| Some(boundary.clone())),
    }
}

fn managed_node_modules_item_root(path: &Path) -> Option<Option<PathBuf>> {
    let components = path.components().collect::<Vec<_>>();
    let node_modules_index =
        components
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, component)| {
                (component.as_os_str() == "node_modules").then_some(index)
            })?;
    let item_index = node_modules_index + 1;
    if item_index >= components.len() {
        return Some(Some(path_prefix(path, node_modules_index + 1)));
    }

    let item_name = components[item_index].as_os_str().to_string_lossy();
    if item_name.starts_with('.') {
        return Some(None);
    }

    if item_name.starts_with('@') {
        let package_index = item_index + 1;
        if package_index >= components.len() {
            return Some(Some(path_prefix(path, item_index + 1)));
        }

        let package_name = components[package_index].as_os_str().to_string_lossy();
        if package_name.starts_with('.') {
            return Some(None);
        }
        return Some(Some(path_prefix(path, package_index + 1)));
    }

    Some(Some(path_prefix(path, item_index + 1)))
}

fn nearest_package_root_within(path: &Path, boundary: &Path) -> Option<PathBuf> {
    let mut current = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };

    loop {
        if !current.starts_with(boundary) {
            return None;
        }
        if current.join("package.json").exists() {
            return Some(current);
        }
        if current == boundary || !current.pop() {
            return None;
        }
    }
}

fn path_prefix(path: &Path, len: usize) -> PathBuf {
    let mut prefix = PathBuf::new();
    for component in path.components().take(len) {
        prefix.push(component.as_os_str());
    }
    prefix
}

fn has_component(path: &Path, name: &str) -> bool {
    path.components()
        .any(|component| component.as_os_str() == name)
}

fn path_file_name(path: &Path) -> Option<&str> {
    path.file_name()?.to_str()
}

fn normalize_path_for_matching(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn hash_bytes(source: &[u8]) -> u64 {
    let mut hasher = StableHasher::new();
    hasher.write(source);
    hasher.finish()
}

struct StableHasher {
    hash: u64,
}

impl StableHasher {
    fn new() -> Self {
        Self {
            hash: 0xcbf29ce484222325,
        }
    }

    fn write(&mut self, source: &[u8]) {
        for byte in source {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(0x100000001b3);
        }
    }

    fn write_str(&mut self, source: &str) {
        self.write(source.as_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn finish(self) -> u64 {
        self.hash
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use filetime::{FileTime, set_file_mtime};

    use super::*;

    #[tokio::test]
    async fn file_system_info_validates_aggregate_file_snapshots()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("module.js");
        write(&path, "export const value = 'before';")?;
        let source = fs::read_to_string(&path)?;
        let file_system_info = FileSystemInfo::new();
        let snapshot = file_system_info
            .create_file_snapshot(&path, &source, SnapshotStrategy::hash())
            .await?;

        assert!(
            file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::hash())
                .await
        );

        write(&path, "export const value = 'after';")?;

        assert!(
            !file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::hash())
                .await
        );

        Ok(())
    }

    #[tokio::test]
    async fn missing_existence_snapshots_invalidate_when_path_appears()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let missing = temp.path().join("dep.ts");
        let file_system_info = FileSystemInfo::new();
        let snapshot = file_system_info
            .create_resolve_snapshot(
                Vec::new(),
                Vec::new(),
                vec![missing.clone()],
                SnapshotStrategy::timestamp(),
            )
            .await?;

        assert!(
            file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::timestamp())
                .await
        );

        write(&missing, "export const value = 'ts';")?;

        assert!(
            !file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::timestamp())
                .await
        );

        Ok(())
    }

    #[tokio::test]
    async fn context_timestamp_snapshots_include_child_names()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let context = temp.path().join("src");
        write(context.join("dep.js"), "export const value = 'js';")?;
        let original_mtime = FileTime::from_system_time(fs::metadata(&context)?.modified()?);
        let file_system_info = FileSystemInfo::new();
        let snapshot = file_system_info
            .create_resolve_snapshot(
                Vec::new(),
                vec![context.clone()],
                Vec::new(),
                SnapshotStrategy::timestamp(),
            )
            .await?;

        write(context.join("dep.ts"), "export const value = 'ts';")?;
        set_file_mtime(&context, original_mtime)?;

        assert!(
            !file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::timestamp())
                .await
        );

        Ok(())
    }

    #[tokio::test]
    async fn context_timestamp_snapshots_include_nested_child_timestamps()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let context = temp.path().join("src");
        let nested = context.join("nested");
        let dependency = nested.join("dep.js");
        write(&dependency, "export const value = 'before';")?;
        let context_mtime = FileTime::from_system_time(fs::metadata(&context)?.modified()?);
        let nested_mtime = FileTime::from_system_time(fs::metadata(&nested)?.modified()?);
        let file_system_info = FileSystemInfo::new();
        let snapshot = file_system_info
            .create_resolve_snapshot(
                Vec::new(),
                vec![context.clone()],
                Vec::new(),
                SnapshotStrategy::timestamp(),
            )
            .await?;

        set_file_mtime(&dependency, FileTime::from_unix_time(2_000_000_000, 0))?;
        set_file_mtime(&nested, nested_mtime)?;
        set_file_mtime(&context, context_mtime)?;

        assert!(
            !file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::timestamp())
                .await
        );

        Ok(())
    }

    #[tokio::test]
    async fn context_hash_snapshots_include_nested_child_content()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let context = temp.path().join("src");
        let dependency = context.join("nested/dep.js");
        write(&dependency, "export const value = 'before';")?;
        let original_mtime = FileTime::from_system_time(fs::metadata(&dependency)?.modified()?);
        let file_system_info = FileSystemInfo::new();
        let snapshot = file_system_info
            .create_resolve_snapshot(
                Vec::new(),
                vec![context.clone()],
                Vec::new(),
                SnapshotStrategy::hash(),
            )
            .await?;

        write(&dependency, "export const value = 'after';")?;
        set_file_mtime(&dependency, original_mtime)?;

        assert!(
            !file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::hash())
                .await
        );

        Ok(())
    }

    #[tokio::test]
    async fn default_managed_node_modules_snapshots_follow_package_version()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let module = temp.path().join("node_modules/pkg/index.js");
        let package_json = temp.path().join("node_modules/pkg/package.json");
        write(&module, "export const value = 'before';")?;
        write(&package_json, r#"{"name":"pkg","version":"1.0.0"}"#)?;
        let source = fs::read_to_string(&module)?;
        let file_system_info = FileSystemInfo::new();
        let snapshot = file_system_info
            .create_file_snapshot(&module, &source, SnapshotStrategy::hash())
            .await?;

        write(&module, "export const value = 'after';")?;
        assert!(
            file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::hash())
                .await
        );

        write(&package_json, r#"{"name":"pkg","version":"2.0.0"}"#)?;
        assert!(
            !file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::hash())
                .await
        );

        Ok(())
    }

    #[tokio::test]
    async fn unmanaged_paths_override_default_managed_paths()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let package_root = temp.path().join("node_modules/pkg");
        let module = package_root.join("index.js");
        write(&module, "export const value = 'before';")?;
        write(
            package_root.join("package.json"),
            r#"{"name":"pkg","version":"1.0.0"}"#,
        )?;

        let mut options = SnapshotOptions::default();
        options
            .unmanaged_paths
            .push(SnapshotPathPattern::Path(package_root));
        let file_system_info = FileSystemInfo::from_snapshot_options(&options);
        let source = fs::read_to_string(&module)?;
        let snapshot = file_system_info
            .create_file_snapshot(&module, &source, SnapshotStrategy::hash())
            .await?;

        write(&module, "export const value = 'after';")?;
        assert!(
            !file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::hash())
                .await
        );

        Ok(())
    }

    #[tokio::test]
    async fn custom_managed_paths_do_not_use_package_roots_above_the_matched_path()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let managed_path = temp.path().join("managed");
        let module = managed_path.join("index.js");
        let root_package_json = temp.path().join("package.json");
        write(&module, "export const value = 'before';")?;
        write(
            &root_package_json,
            r#"{"name":"workspace","version":"1.0.0"}"#,
        )?;

        let mut options = SnapshotOptions::default();
        options.managed_paths = vec![SnapshotPathPattern::Path(managed_path.clone())];
        let file_system_info = FileSystemInfo::from_snapshot_options(&options);
        let source = fs::read_to_string(&module)?;
        let snapshot = file_system_info
            .create_file_snapshot(&module, &source, SnapshotStrategy::hash())
            .await?;

        write(
            &root_package_json,
            r#"{"name":"workspace","version":"2.0.0"}"#,
        )?;
        assert!(
            file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::hash())
                .await
        );

        write(
            managed_path.join("package.json"),
            r#"{"name":"managed","version":"1.0.0"}"#,
        )?;
        assert!(
            !file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::hash())
                .await
        );

        Ok(())
    }

    #[tokio::test]
    async fn managed_regex_paths_use_the_first_capture_as_the_managed_item()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let package_root = temp.path().join("managed/pkg");
        let module = package_root.join("index.js");
        let package_json = package_root.join("package.json");
        write(&module, "export const value = 'before';")?;
        write(&package_json, r#"{"name":"pkg","version":"1.0.0"}"#)?;

        let mut options = SnapshotOptions::default();
        options.managed_paths = vec![SnapshotPathPattern::Regex {
            source: format!(
                "({})",
                regex::escape(&normalize_path_for_matching(&package_root))
            ),
            flags: String::new(),
        }];
        let file_system_info = FileSystemInfo::from_snapshot_options(&options);
        let source = fs::read_to_string(&module)?;
        let snapshot = file_system_info
            .create_file_snapshot(&module, &source, SnapshotStrategy::hash())
            .await?;

        write(&module, "export const value = 'after';")?;
        assert!(
            file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::hash())
                .await
        );

        write(&package_json, r#"{"name":"pkg","version":"2.0.0"}"#)?;
        assert!(
            !file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::hash())
                .await
        );

        Ok(())
    }

    #[tokio::test]
    async fn invalid_managed_packages_fall_back_to_file_snapshots()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let module = temp.path().join("node_modules/pkg/index.js");
        write(&module, "export const value = 'before';")?;
        write(
            temp.path().join("node_modules/pkg/package.json"),
            r#"{"name":"pkg"}"#,
        )?;
        let source = fs::read_to_string(&module)?;
        let file_system_info = FileSystemInfo::new();
        let snapshot = file_system_info
            .create_file_snapshot(&module, &source, SnapshotStrategy::hash())
            .await?;

        write(&module, "export const value = 'after';")?;
        assert!(
            !file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::hash())
                .await
        );

        Ok(())
    }

    #[tokio::test]
    async fn immutable_regex_paths_are_recorded_without_file_validation()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let module = temp.path().join("node_modules/pkg/index.js");
        write(&module, "export const value = 'before';")?;

        let mut options = SnapshotOptions::default();
        options.immutable_paths.push(SnapshotPathPattern::Regex {
            source: "NODE_MODULES.PKG".to_string(),
            flags: "i".to_string(),
        });
        let file_system_info = FileSystemInfo::from_snapshot_options(&options);
        let source = fs::read_to_string(&module)?;
        let snapshot = file_system_info
            .create_file_snapshot(&module, &source, SnapshotStrategy::hash())
            .await?;

        write(&module, "export const value = 'after';")?;
        assert!(
            file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::hash())
                .await
        );

        Ok(())
    }

    #[tokio::test]
    async fn file_system_info_merges_snapshots_with_path_union_and_later_override()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let shared = temp.path().join("shared.js");
        let extra = temp.path().join("extra.js");
        let file_system_info = FileSystemInfo::new();

        write(&shared, "export const value = 'before';")?;
        let stale_shared = file_system_info
            .create_snapshot(vec![shared.clone()], SnapshotStrategy::hash())
            .await?;
        write(&shared, "export const value = 'after';")?;
        let fresh_shared = file_system_info
            .create_snapshot(vec![shared.clone()], SnapshotStrategy::hash())
            .await?;
        write(&extra, "export const extra = true;")?;
        let extra_snapshot = file_system_info
            .create_snapshot(vec![extra.clone()], SnapshotStrategy::hash())
            .await?;

        let merged =
            file_system_info.merge_snapshots([&stale_shared, &extra_snapshot, &fresh_shared]);

        assert!(
            file_system_info
                .is_snapshot_valid(&merged, SnapshotStrategy::hash())
                .await
        );

        write(&extra, "export const extra = false;")?;

        assert!(
            !file_system_info
                .is_snapshot_valid(&merged, SnapshotStrategy::hash())
                .await
        );

        Ok(())
    }

    fn write(path: impl AsRef<Path>, source: &str) -> std::io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, source)
    }
}
