use std::{
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
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self {
            module: SnapshotStrategy::timestamp(),
            resolve: SnapshotStrategy::timestamp(),
            build_dependencies: SnapshotStrategy::timestamp_and_hash(),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FileSnapshot {
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
pub(crate) struct FileSetSnapshot {
    files: Vec<SnapshottedFile>,
}

impl FileSetSnapshot {
    pub(crate) async fn create(
        paths: impl IntoIterator<Item = PathBuf>,
        strategy: SnapshotStrategy,
    ) -> Result<Self> {
        let mut paths = paths.into_iter().collect::<Vec<_>>();
        paths.sort();
        paths.dedup();

        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            files.push(SnapshottedFile {
                snapshot: FileSnapshot::create_from_path(&path, strategy).await?,
                path,
            });
        }
        Ok(Self { files })
    }

    pub(crate) async fn is_valid(&self, strategy: SnapshotStrategy) -> bool {
        for file in &self.files {
            if !file.snapshot.is_valid(&file.path, strategy).await {
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

fn hash_bytes(source: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    for byte in source {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
