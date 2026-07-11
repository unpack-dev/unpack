//! Public Cache options and defaults consumed by the Build Cache composition root.

use std::{path::PathBuf, time::Duration};

use crate::pack_file::{DEFAULT_MAX_AGE, PackFileCompression};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheOptions {
    pub kind: CacheKind,
    pub cache_directory: Option<PathBuf>,
    pub cache_location: Option<PathBuf>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub build_dependencies: Vec<BuildDependency>,
    pub automatic_build_dependencies: Vec<PathBuf>,
    pub max_age: Duration,
    pub max_memory_generations: Option<u64>,
    pub compression: CacheCompression,
    pub allow_collecting_memory: bool,
    pub idle_timeout: Option<u32>,
    pub idle_timeout_for_initial_store: Option<u32>,
    pub idle_timeout_after_large_changes: Option<u32>,
    pub profile: bool,
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
            automatic_build_dependencies: Vec::new(),
            max_age: DEFAULT_MAX_AGE,
            compression: CacheCompression::None,
            allow_collecting_memory: false,
            max_memory_generations: None,
            idle_timeout: None,
            idle_timeout_for_initial_store: None,
            idle_timeout_after_large_changes: None,
            profile: false,
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
            automatic_build_dependencies: Vec::new(),
            max_age: DEFAULT_MAX_AGE,
            compression: CacheCompression::None,
            allow_collecting_memory: false,
            max_memory_generations: None,
            idle_timeout: None,
            idle_timeout_for_initial_store: None,
            idle_timeout_after_large_changes: None,
            profile: false,
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
            automatic_build_dependencies: Vec::new(),
            max_age: DEFAULT_MAX_AGE,
            compression: CacheCompression::None,
            allow_collecting_memory: false,
            max_memory_generations: None,
            idle_timeout: Some(60_000),
            idle_timeout_for_initial_store: Some(5_000),
            idle_timeout_after_large_changes: Some(1_000),
            profile: false,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheCompression {
    None,
    Gzip,
    Brotli,
}

impl From<CacheCompression> for PackFileCompression {
    fn from(compression: CacheCompression) -> Self {
        match compression {
            CacheCompression::None => Self::None,
            CacheCompression::Gzip => Self::Gzip,
            CacheCompression::Brotli => Self::Brotli,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildDependency {
    pub name: String,
    pub requests: Vec<String>,
}
