use std::{
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use regex::RegexBuilder;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
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
            managed_paths: Vec::new(),
            immutable_paths: Vec::new(),
            unmanaged_paths: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotPathPattern {
    Path(PathBuf),
    Regex {
        source: String,
        case_insensitive: bool,
    },
}

impl SnapshotPathPattern {
    pub fn path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self::Path(fs::canonicalize(&path).unwrap_or(path))
    }

    pub fn regex(source: impl Into<String>, case_insensitive: bool) -> Self {
        Self::Regex {
            source: source.into(),
            case_insensitive,
        }
    }

    fn matches(&self, path: &Path) -> bool {
        match self {
            Self::Path(pattern) => path == pattern || path.starts_with(pattern),
            Self::Regex {
                source,
                case_insensitive,
            } => RegexBuilder::new(source)
                .case_insensitive(*case_insensitive)
                .build()
                .map(|regex| regex.is_match(&normalize_path_for_pattern(path)))
                .unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default)]
pub(crate) struct FileSystemInfo {
    path_options: SnapshotPathOptions,
}

impl FileSystemInfo {
    pub(crate) fn new(options: &SnapshotOptions) -> Self {
        Self {
            path_options: SnapshotPathOptions::from_options(options),
        }
    }

    pub(crate) async fn create_file_snapshot(
        &self,
        path: &Path,
        source: &str,
        strategy: SnapshotStrategy,
    ) -> Result<Snapshot> {
        Snapshot::create_file(path, source, strategy, &self.path_options).await
    }

    pub(crate) async fn create_resolve_snapshot(
        &self,
        file_dependencies: impl IntoIterator<Item = PathBuf>,
        context_dependencies: impl IntoIterator<Item = PathBuf>,
        missing_dependencies: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
    ) -> Result<Snapshot> {
        Snapshot::create_resolve(
            file_dependencies,
            context_dependencies,
            missing_dependencies,
            strategy,
            &self.path_options,
        )
        .await
    }

    pub(crate) fn create_snapshot_sync(
        &self,
        paths: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
    ) -> Result<Snapshot> {
        Snapshot::create_sync(paths, strategy, &self.path_options)
    }

    pub(crate) async fn is_snapshot_valid(
        &self,
        snapshot: &Snapshot,
        strategy: SnapshotStrategy,
    ) -> bool {
        snapshot.is_valid(strategy).await
    }

    pub(crate) fn is_snapshot_valid_sync(
        &self,
        snapshot: &Snapshot,
        strategy: SnapshotStrategy,
    ) -> bool {
        snapshot.is_valid_sync(strategy)
    }
}

#[derive(Debug, Clone, Default)]
struct SnapshotPathOptions {
    managed_paths: Vec<SnapshotPathPattern>,
    immutable_paths: Vec<SnapshotPathPattern>,
    unmanaged_paths: Vec<SnapshotPathPattern>,
}

impl SnapshotPathOptions {
    fn from_options(options: &SnapshotOptions) -> Self {
        Self {
            managed_paths: options.managed_paths.clone(),
            immutable_paths: options.immutable_paths.clone(),
            unmanaged_paths: options.unmanaged_paths.clone(),
        }
    }

    fn classify(&self, path: &Path) -> PathClassification {
        if self
            .unmanaged_paths
            .iter()
            .any(|pattern| pattern.matches(path))
        {
            return PathClassification::Normal;
        }
        if self
            .immutable_paths
            .iter()
            .any(|pattern| pattern.matches(path))
        {
            return PathClassification::Immutable;
        }
        if self
            .managed_paths
            .iter()
            .any(|pattern| pattern.matches(path))
            || is_default_managed_path(path)
        {
            return PathClassification::Managed;
        }
        PathClassification::Normal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathClassification {
    Normal,
    Immutable,
    Managed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    files: Vec<SnapshottedFile>,
    contexts: Vec<SnapshottedContext>,
    missing: Vec<MissingSnapshot>,
    managed_items: Vec<ManagedItemSnapshot>,
    immutable_paths: Vec<PathBuf>,
}

impl Snapshot {
    async fn create_file(
        path: &Path,
        source: &str,
        strategy: SnapshotStrategy,
        path_options: &SnapshotPathOptions,
    ) -> Result<Self> {
        let mut snapshot = Self::default();
        snapshot
            .add_existing_file(path, Some(source), strategy, path_options)
            .await?;
        snapshot.sort();
        Ok(snapshot)
    }

    async fn create_resolve(
        file_dependencies: impl IntoIterator<Item = PathBuf>,
        context_dependencies: impl IntoIterator<Item = PathBuf>,
        missing_dependencies: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
        path_options: &SnapshotPathOptions,
    ) -> Result<Self> {
        let mut snapshot = Self::default();
        for path in normalize_paths(file_dependencies) {
            snapshot.add_path(&path, strategy, path_options).await?;
        }
        for path in normalize_paths(context_dependencies) {
            snapshot.add_context(&path, strategy, path_options).await?;
        }
        for path in normalize_paths(missing_dependencies) {
            snapshot.add_missing(path);
        }
        snapshot.sort();
        Ok(snapshot)
    }

    fn create_sync(
        paths: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
        path_options: &SnapshotPathOptions,
    ) -> Result<Self> {
        let mut snapshot = Self::default();
        for path in normalize_paths(paths) {
            snapshot.add_path_sync(&path, strategy, path_options)?;
        }
        snapshot.sort();
        Ok(snapshot)
    }

    async fn add_path(
        &mut self,
        path: &Path,
        strategy: SnapshotStrategy,
        path_options: &SnapshotPathOptions,
    ) -> Result<()> {
        let metadata = match tokio::fs::metadata(path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.add_missing(path.to_path_buf());
                return Ok(());
            }
            Err(error) => return Err(Error::read(path, error)),
        };
        if metadata.is_dir() {
            self.add_context(path, strategy, path_options).await
        } else {
            self.add_existing_file(path, None, strategy, path_options)
                .await
        }
    }

    async fn add_existing_file(
        &mut self,
        path: &Path,
        source: Option<&str>,
        strategy: SnapshotStrategy,
        path_options: &SnapshotPathOptions,
    ) -> Result<()> {
        match path_options.classify(path) {
            PathClassification::Normal => {
                let snapshot = match source {
                    Some(source) => FileSnapshot::create(path, source, strategy).await?,
                    None => FileSnapshot::create_from_path(path, strategy).await?,
                };
                self.files.push(SnapshottedFile {
                    path: path.to_path_buf(),
                    snapshot,
                });
            }
            PathClassification::Immutable => {
                self.immutable_paths.push(path.to_path_buf());
            }
            PathClassification::Managed => {
                self.managed_items
                    .push(ManagedItemSnapshot::create(path).await?);
            }
        }
        Ok(())
    }

    async fn add_context(
        &mut self,
        path: &Path,
        strategy: SnapshotStrategy,
        path_options: &SnapshotPathOptions,
    ) -> Result<()> {
        match path_options.classify(path) {
            PathClassification::Normal => {
                self.contexts.push(SnapshottedContext {
                    path: path.to_path_buf(),
                    snapshot: ContextSnapshot::create(path, strategy).await?,
                });
            }
            PathClassification::Immutable => {
                self.immutable_paths.push(path.to_path_buf());
            }
            PathClassification::Managed => {
                self.managed_items
                    .push(ManagedItemSnapshot::create(path).await?);
            }
        }
        Ok(())
    }

    fn add_path_sync(
        &mut self,
        path: &Path,
        strategy: SnapshotStrategy,
        path_options: &SnapshotPathOptions,
    ) -> Result<()> {
        let metadata = fs::metadata(path).map_err(|error| Error::read(path, error))?;
        if metadata.is_dir() {
            self.add_context_sync(path, strategy, path_options)
        } else {
            self.add_existing_file_sync(path, strategy, path_options)
        }
    }

    fn add_existing_file_sync(
        &mut self,
        path: &Path,
        strategy: SnapshotStrategy,
        path_options: &SnapshotPathOptions,
    ) -> Result<()> {
        match path_options.classify(path) {
            PathClassification::Normal => {
                self.files.push(SnapshottedFile {
                    path: path.to_path_buf(),
                    snapshot: FileSnapshot::create_from_file_sync(path, strategy)?,
                });
            }
            PathClassification::Immutable => {
                self.immutable_paths.push(path.to_path_buf());
            }
            PathClassification::Managed => {
                self.managed_items
                    .push(ManagedItemSnapshot::create_sync(path)?);
            }
        }
        Ok(())
    }

    fn add_context_sync(
        &mut self,
        path: &Path,
        strategy: SnapshotStrategy,
        path_options: &SnapshotPathOptions,
    ) -> Result<()> {
        match path_options.classify(path) {
            PathClassification::Normal => {
                self.contexts.push(SnapshottedContext {
                    path: path.to_path_buf(),
                    snapshot: ContextSnapshot::create_sync(path, strategy)?,
                });
            }
            PathClassification::Immutable => {
                self.immutable_paths.push(path.to_path_buf());
            }
            PathClassification::Managed => {
                self.managed_items
                    .push(ManagedItemSnapshot::create_sync(path)?);
            }
        }
        Ok(())
    }

    fn add_missing(&mut self, path: PathBuf) {
        self.missing.push(MissingSnapshot { path });
    }

    async fn is_valid(&self, strategy: SnapshotStrategy) -> bool {
        for file in &self.files {
            if !file.snapshot.is_valid(&file.path, strategy).await {
                return false;
            }
        }
        for context in &self.contexts {
            if !context.snapshot.is_valid(&context.path, strategy).await {
                return false;
            }
        }
        for missing in &self.missing {
            if !missing.is_valid().await {
                return false;
            }
        }
        for managed_item in &self.managed_items {
            if !managed_item.is_valid().await {
                return false;
            }
        }
        true
    }

    fn is_valid_sync(&self, strategy: SnapshotStrategy) -> bool {
        for file in &self.files {
            if !file.snapshot.is_valid_sync(&file.path, strategy) {
                return false;
            }
        }
        for context in &self.contexts {
            if !context.snapshot.is_valid_sync(&context.path, strategy) {
                return false;
            }
        }
        for missing in &self.missing {
            if !missing.is_valid_sync() {
                return false;
            }
        }
        for managed_item in &self.managed_items {
            if !managed_item.is_valid_sync() {
                return false;
            }
        }
        true
    }

    fn sort(&mut self) {
        self.files.sort_by(|left, right| left.path.cmp(&right.path));
        self.files.dedup_by(|left, right| left.path == right.path);
        self.contexts
            .sort_by(|left, right| left.path.cmp(&right.path));
        self.contexts
            .dedup_by(|left, right| left.path == right.path);
        self.missing
            .sort_by(|left, right| left.path.cmp(&right.path));
        self.missing.dedup_by(|left, right| left.path == right.path);
        self.managed_items
            .sort_by(|left, right| left.item_path.cmp(&right.item_path));
        self.managed_items
            .dedup_by(|left, right| left.item_path == right.item_path);
        self.immutable_paths.sort();
        self.immutable_paths.dedup();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FileSnapshot {
    exists: bool,
    modified: Option<SystemTime>,
    source_hash: Option<u64>,
}

impl FileSnapshot {
    async fn create(path: &Path, source: &str, strategy: SnapshotStrategy) -> Result<Self> {
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

    async fn create_from_path(path: &Path, strategy: SnapshotStrategy) -> Result<Self> {
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

    async fn is_valid(&self, path: &Path, strategy: SnapshotStrategy) -> bool {
        if !self.exists {
            return matches!(
                tokio::fs::metadata(path).await,
                Err(error) if error.kind() == io::ErrorKind::NotFound
            );
        }

        if strategy.timestamp {
            let Ok(metadata) = tokio::fs::metadata(path).await else {
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
            let Ok(source) = tokio::fs::read(path).await else {
                return false;
            };
            if Some(hash_bytes(&source)) != self.source_hash {
                return false;
            }
        }

        true
    }

    fn create_from_file_sync(path: &Path, strategy: SnapshotStrategy) -> Result<Self> {
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

    fn create_from_path_sync(path: &Path, strategy: SnapshotStrategy) -> Result<Self> {
        let metadata = match fs::metadata(path) {
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
            Some(hash_bytes(
                &fs::read(path).map_err(|error| Error::read(path, error))?,
            ))
        } else {
            None
        };
        Ok(Self {
            exists: true,
            modified,
            source_hash,
        })
    }

    fn is_valid_sync(&self, path: &Path, strategy: SnapshotStrategy) -> bool {
        if !self.exists {
            return matches!(fs::metadata(path), Err(error) if error.kind() == io::ErrorKind::NotFound);
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
struct ContextSnapshot {
    exists: bool,
    modified: Option<SystemTime>,
    entries_hash: Option<u64>,
}

impl ContextSnapshot {
    async fn create(path: &Path, strategy: SnapshotStrategy) -> Result<Self> {
        let metadata = match tokio::fs::metadata(path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self {
                    exists: false,
                    modified: None,
                    entries_hash: None,
                });
            }
            Err(error) => return Err(Error::read(path, error)),
        };
        if !metadata.is_dir() {
            return Ok(Self {
                exists: false,
                modified: None,
                entries_hash: None,
            });
        }
        let modified = if strategy.timestamp {
            Some(
                metadata
                    .modified()
                    .map_err(|error| Error::read(path, error))?,
            )
        } else {
            None
        };
        let entries_hash = (strategy.timestamp || strategy.hash)
            .then(|| directory_entries_hash(path))
            .transpose()?;
        Ok(Self {
            exists: true,
            modified,
            entries_hash,
        })
    }

    fn create_sync(path: &Path, strategy: SnapshotStrategy) -> Result<Self> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self {
                    exists: false,
                    modified: None,
                    entries_hash: None,
                });
            }
            Err(error) => return Err(Error::read(path, error)),
        };
        if !metadata.is_dir() {
            return Ok(Self {
                exists: false,
                modified: None,
                entries_hash: None,
            });
        }
        let modified = if strategy.timestamp {
            Some(
                metadata
                    .modified()
                    .map_err(|error| Error::read(path, error))?,
            )
        } else {
            None
        };
        let entries_hash = (strategy.timestamp || strategy.hash)
            .then(|| directory_entries_hash(path))
            .transpose()?;
        Ok(Self {
            exists: true,
            modified,
            entries_hash,
        })
    }

    async fn is_valid(&self, path: &Path, strategy: SnapshotStrategy) -> bool {
        if !self.exists {
            return matches!(
                tokio::fs::metadata(path).await,
                Err(error) if error.kind() == io::ErrorKind::NotFound
            );
        }
        let Ok(metadata) = tokio::fs::metadata(path).await else {
            return false;
        };
        if !metadata.is_dir() {
            return false;
        }
        if strategy.timestamp {
            let Ok(modified) = metadata.modified() else {
                return false;
            };
            if Some(modified) != self.modified {
                return false;
            }
        }
        if self.entries_hash.is_some() {
            let Ok(entries_hash) = directory_entries_hash(path) else {
                return false;
            };
            if Some(entries_hash) != self.entries_hash {
                return false;
            }
        }
        true
    }

    fn is_valid_sync(&self, path: &Path, strategy: SnapshotStrategy) -> bool {
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
            let Ok(modified) = metadata.modified() else {
                return false;
            };
            if Some(modified) != self.modified {
                return false;
            }
        }
        if self.entries_hash.is_some() {
            let Ok(entries_hash) = directory_entries_hash(path) else {
                return false;
            };
            if Some(entries_hash) != self.entries_hash {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MissingSnapshot {
    path: PathBuf,
}

impl MissingSnapshot {
    async fn is_valid(&self) -> bool {
        matches!(
            tokio::fs::metadata(&self.path).await,
            Err(error) if error.kind() == io::ErrorKind::NotFound
        )
    }

    fn is_valid_sync(&self) -> bool {
        matches!(fs::metadata(&self.path), Err(error) if error.kind() == io::ErrorKind::NotFound)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ManagedItemSnapshot {
    item_path: PathBuf,
    package_json: PathBuf,
    package_snapshot: FileSnapshot,
}

impl ManagedItemSnapshot {
    async fn create(path: &Path) -> Result<Self> {
        let item_path = managed_item_path(path);
        let package_json = item_path.join("package.json");
        let package_snapshot =
            FileSnapshot::create_from_path(&package_json, SnapshotStrategy::timestamp_and_hash())
                .await?;
        Ok(Self {
            item_path,
            package_json,
            package_snapshot,
        })
    }

    fn create_sync(path: &Path) -> Result<Self> {
        let item_path = managed_item_path(path);
        let package_json = item_path.join("package.json");
        let package_snapshot = FileSnapshot::create_from_path_sync(
            &package_json,
            SnapshotStrategy::timestamp_and_hash(),
        )?;
        Ok(Self {
            item_path,
            package_json,
            package_snapshot,
        })
    }

    async fn is_valid(&self) -> bool {
        self.package_snapshot
            .is_valid(&self.package_json, SnapshotStrategy::timestamp_and_hash())
            .await
    }

    fn is_valid_sync(&self) -> bool {
        self.package_snapshot
            .is_valid_sync(&self.package_json, SnapshotStrategy::timestamp_and_hash())
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

fn is_default_managed_path(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == OsStr::new("node_modules"))
}

fn managed_item_path(path: &Path) -> PathBuf {
    let start = path.parent().unwrap_or(path);
    for ancestor in start.ancestors() {
        if ancestor.join("package.json").exists() {
            return ancestor.to_path_buf();
        }
    }
    node_modules_item_path(path).unwrap_or_else(|| start.to_path_buf())
}

fn node_modules_item_path(path: &Path) -> Option<PathBuf> {
    let components = path.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate().rev() {
        if component.as_os_str() != OsStr::new("node_modules") {
            continue;
        }
        let Some(package_component) = components.get(index + 1) else {
            continue;
        };
        let mut end = index + 2;
        if package_component
            .as_os_str()
            .to_string_lossy()
            .starts_with('@')
        {
            end += 1;
        }
        if end > components.len() {
            continue;
        }
        let mut item = PathBuf::new();
        for component in components.iter().take(end) {
            item.push(component.as_os_str());
        }
        return Some(item);
    }
    None
}

fn normalize_path_for_pattern(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn directory_entries_hash(path: &Path) -> Result<u64> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path).map_err(|error| Error::read(path, error))? {
        let entry = entry.map_err(|error| Error::read(path, error))?;
        let file_type = entry
            .file_type()
            .map_err(|error| Error::read(entry.path(), error))?;
        let kind = if file_type.is_dir() {
            "d"
        } else if file_type.is_file() {
            "f"
        } else if file_type.is_symlink() {
            "s"
        } else {
            "o"
        };
        entries.push(format!("{}:{kind}", entry.file_name().to_string_lossy()));
    }
    entries.sort();
    Ok(hash_bytes(entries.join("\n").as_bytes()))
}

fn hash_bytes(source: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    for byte in source {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::*;

    #[tokio::test]
    async fn file_system_info_validates_aggregate_file_snapshots()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("module.js");
        write(&path, "export const value = 'before';")?;
        let source = fs::read_to_string(&path)?;
        let snapshot_options = SnapshotOptions::default();
        let file_system_info = FileSystemInfo::new(&snapshot_options);
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
    async fn managed_item_snapshots_follow_package_metadata()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let package = temp.path().join("node_modules/package");
        let module = package.join("index.js");
        let manifest = package.join("package.json");
        write(&module, "export const value = 'before';")?;
        write(&manifest, r#"{"name":"package","version":"1.0.0"}"#)?;
        let source = fs::read_to_string(&module)?;
        let snapshot_options = SnapshotOptions::default();
        let file_system_info = FileSystemInfo::new(&snapshot_options);
        let snapshot = file_system_info
            .create_file_snapshot(&module, &source, SnapshotStrategy::timestamp())
            .await?;

        write(&module, "export const value = 'after';")?;
        assert!(
            file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::timestamp())
                .await
        );

        write(&manifest, r#"{"name":"package","version":"2.0.0"}"#)?;
        assert!(
            !file_system_info
                .is_snapshot_valid(&snapshot, SnapshotStrategy::timestamp())
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
