use std::{path::Path, time::SystemTime};

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotOptions {
    pub module: SnapshotStrategy,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self {
            module: SnapshotStrategy::timestamp(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileSnapshot {
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
        let source_hash = strategy.hash.then(|| hash_source(source));

        Ok(Self {
            modified,
            source_hash,
        })
    }

    pub(crate) async fn is_valid(&self, path: &Path, strategy: SnapshotStrategy) -> bool {
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
            let Ok(source) = tokio::fs::read_to_string(path).await else {
                return false;
            };
            if Some(hash_source(&source)) != self.source_hash {
                return false;
            }
        }

        true
    }
}

fn hash_source(source: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    for byte in source.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
