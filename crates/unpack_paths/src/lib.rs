#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::{
    collections::{HashMap, HashSet},
    fmt,
    hash::{BuildHasherDefault, Hash, Hasher},
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
};

use rspack_resolver::ResolverPath;
use rustc_hash::FxHasher;

/// An immutable, cheaply cloned path with a hash computed at construction.
#[derive(Clone)]
pub struct ArcPath {
    path: Arc<Path>,
    hash: u64,
}

impl ArcPath {
    #[inline]
    pub fn new(path: Arc<Path>) -> Self {
        let hash = hash_path(&path);
        Self { path, hash }
    }

    /// Constructs a path from a hash produced by [`hash_path`].
    ///
    /// Callers must ensure that `hash` belongs to `path`.
    #[inline]
    fn from_parts(hash: u64, path: Arc<Path>) -> Self {
        Self { path, hash }
    }

    #[inline]
    pub fn precomputed_hash(&self) -> u64 {
        self.hash
    }

    #[inline]
    pub fn into_arc(self) -> Arc<Path> {
        self.path
    }
}

/// Lexically normalizes a path without accessing the file system.
pub fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir if normalized.pop() => {}
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalize_platform_path(normalized)
}

#[cfg(target_os = "macos")]
fn normalize_platform_path(path: PathBuf) -> PathBuf {
    if let Ok(relative) = path.strip_prefix("/var") {
        return PathBuf::from("/private/var").join(relative);
    }
    path
}

#[cfg(not(target_os = "macos"))]
fn normalize_platform_path(path: PathBuf) -> PathBuf {
    path
}

/// Hashes the platform representation directly, matching `ResolverPath`.
#[inline]
pub fn hash_path(path: &Path) -> u64 {
    let mut hasher = FxHasher::default();
    #[cfg(unix)]
    hasher.write(path.as_os_str().as_bytes());
    #[cfg(not(unix))]
    path.hash(&mut hasher);
    hasher.finish()
}

impl PartialEq for ArcPath {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.path, &other.path) || (self.hash == other.hash && self.path == other.path)
    }
}

impl Eq for ArcPath {}

impl Hash for ArcPath {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl fmt::Debug for ArcPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.path.fmt(f)
    }
}

impl Deref for ArcPath {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

impl AsRef<Path> for ArcPath {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl From<PathBuf> for ArcPath {
    fn from(path: PathBuf) -> Self {
        Self::new(path.into())
    }
}

impl From<&Path> for ArcPath {
    fn from(path: &Path) -> Self {
        Self::new(path.into())
    }
}

impl From<ResolverPath> for ArcPath {
    fn from(path: ResolverPath) -> Self {
        Self::from_parts(path.precomputed_hash(), path.into_arc())
    }
}

/// A hasher for keys whose `Hash` implementation writes one precomputed `u64`.
#[derive(Default)]
pub struct PrecomputedHasher(u64);

impl Hasher for PrecomputedHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, _bytes: &[u8]) {
        panic!("PrecomputedHasher only accepts a precomputed u64")
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = value;
    }
}

pub type ArcPathMap<V> = HashMap<ArcPath, V, BuildHasherDefault<PrecomputedHasher>>;
pub type ArcPathSet = HashSet<ArcPath, BuildHasherDefault<PrecomputedHasher>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_paths_share_hash_and_compare_equal() {
        let first = ArcPath::from(PathBuf::from("/project/src/index.js"));
        let second = ArcPath::from(PathBuf::from("/project/src/index.js"));

        assert_eq!(first.precomputed_hash(), second.precomputed_hash());
        assert_eq!(first, second);
    }

    #[test]
    fn path_set_uses_precomputed_hash() {
        let mut paths = ArcPathSet::default();
        paths.insert(ArcPath::from(PathBuf::from("/project/src/index.js")));

        assert!(paths.contains(&ArcPath::from(PathBuf::from("/project/src/index.js"))));
    }

    #[test]
    fn normalizes_lexical_components() {
        assert_eq!(
            normalize(Path::new("/project/./src/../index.js")),
            PathBuf::from("/project/index.js")
        );
    }
}
