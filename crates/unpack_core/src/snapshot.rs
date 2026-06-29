use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotOptions {
    pub module: SnapshotStrategy,
    pub resolve: SnapshotStrategy,
    pub build_dependencies: SnapshotStrategy,
    pub resolve_build_dependencies: SnapshotStrategy,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self {
            module: SnapshotStrategy::timestamp(),
            resolve: SnapshotStrategy::timestamp(),
            build_dependencies: SnapshotStrategy::timestamp_and_hash(),
            resolve_build_dependencies: SnapshotStrategy::timestamp_and_hash(),
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
pub(crate) struct FileSystemInfo;

impl FileSystemInfo {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) async fn create_file_snapshot(
        &self,
        path: &Path,
        source: &str,
        strategy: SnapshotStrategy,
    ) -> Result<Snapshot> {
        Snapshot::create_file(path, source, strategy).await
    }

    pub(crate) async fn create_snapshot(
        &self,
        paths: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
    ) -> Result<Snapshot> {
        Snapshot::create(paths, strategy).await
    }

    pub(crate) fn create_snapshot_sync(
        &self,
        paths: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
    ) -> Result<Snapshot> {
        Snapshot::create_sync(paths, strategy)
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

    pub(crate) fn merge_snapshots<'a>(
        &self,
        snapshots: impl IntoIterator<Item = &'a Snapshot>,
    ) -> Snapshot {
        Snapshot::merge(snapshots)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    files: Vec<SnapshottedFile>,
}

impl Snapshot {
    async fn create_file(path: &Path, source: &str, strategy: SnapshotStrategy) -> Result<Self> {
        Ok(Self {
            files: vec![SnapshottedFile {
                path: path.to_path_buf(),
                snapshot: FileSnapshot::create(path, source, strategy).await?,
            }],
        })
    }

    async fn create(
        paths: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
    ) -> Result<Self> {
        let paths = normalize_paths(paths);
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            files.push(SnapshottedFile {
                snapshot: FileSnapshot::create_from_path(&path, strategy).await?,
                path,
            });
        }
        Ok(Self { files })
    }

    fn create_sync(
        paths: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
    ) -> Result<Self> {
        let paths = normalize_paths(paths);
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            files.push(SnapshottedFile {
                snapshot: FileSnapshot::create_from_file_sync(&path, strategy)?,
                path,
            });
        }
        Ok(Self { files })
    }

    async fn is_valid(&self, strategy: SnapshotStrategy) -> bool {
        for file in &self.files {
            if !file.snapshot.is_valid(&file.path, strategy).await {
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
        true
    }

    pub(crate) fn has_exact_paths(&self, paths: impl IntoIterator<Item = PathBuf>) -> bool {
        let paths = normalize_paths(paths);
        self.files.len() == paths.len()
            && self
                .files
                .iter()
                .zip(paths.iter())
                .all(|(file, path)| file.path == *path)
    }

    fn merge<'a>(snapshots: impl IntoIterator<Item = &'a Snapshot>) -> Self {
        let mut files = BTreeMap::new();
        for snapshot in snapshots {
            for file in &snapshot.files {
                files.insert(file.path.clone(), file.clone());
            }
        }

        Self {
            files: files.into_values().collect(),
        }
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

    pub(crate) async fn is_valid(&self, path: &Path, strategy: SnapshotStrategy) -> bool {
        if !self.exists {
            return !strategy.timestamp && !strategy.hash
                || matches!(
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
struct SnapshottedFile {
    path: PathBuf,
    snapshot: FileSnapshot,
}

fn normalize_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut paths = paths.into_iter().collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
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
        fs::write(path, source)
    }
}
