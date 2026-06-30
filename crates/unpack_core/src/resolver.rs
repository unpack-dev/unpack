use std::{
    collections::BTreeSet,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{Error, ModuleIdentity, Result};

pub use rspack_resolver::ResolveOptions;

#[derive(Debug, Clone)]
pub struct UnpackResolver {
    inner: Arc<rspack_resolver::Resolver>,
    path_cache: Arc<Mutex<PathResolutionCache>>,
}

#[derive(Debug, Default)]
struct PathResolutionCache {
    normalized: HashMap<PathBuf, PathBuf>,
    is_dir: HashMap<PathBuf, bool>,
}

impl UnpackResolver {
    pub fn new(options: ResolveOptions) -> Self {
        Self {
            inner: Arc::new(rspack_resolver::Resolver::new(options)),
            path_cache: Arc::new(Mutex::new(PathResolutionCache::default())),
        }
    }

    pub async fn resolve(&self, directory: &Path, request: &str) -> Result<ResolvedResource> {
        Ok(self
            .resolve_with_dependencies(directory, request)
            .await?
            .resource)
    }

    pub async fn resolve_with_dependencies(
        &self,
        directory: &Path,
        request: &str,
    ) -> Result<ResolveResult> {
        let mut context = rspack_resolver::ResolveContext::default();
        let resolution = self
            .inner
            .resolve_with_context(directory, request, &mut context)
            .await
            .map_err(|error| Error::resolve(directory, request, error))?;
        let file_dependencies = context
            .file_dependencies
            .into_iter()
            .map(|path| self.normalize_resolver_dependency(path.as_path()))
            .collect::<BTreeSet<_>>();
        let missing_dependencies = context
            .missing_dependencies
            .into_iter()
            .map(|path| self.normalize_resolver_dependency(path.as_path()))
            .collect::<BTreeSet<_>>();
        let context_dependencies =
            self.context_dependencies(directory, &file_dependencies, &missing_dependencies);

        Ok(ResolveResult {
            resource: ResolvedResource {
                path: resolution.path().to_path_buf(),
                query: resolution.query().map(ToOwned::to_owned),
                fragment: resolution.fragment().map(ToOwned::to_owned),
            },
            file_dependencies,
            context_dependencies,
            missing_dependencies,
        })
    }

    fn normalize_resolver_dependency(&self, path: &Path) -> PathBuf {
        if let Some(cached) = self
            .path_cache
            .lock()
            .expect("resolver path cache mutex should not be poisoned")
            .normalized
            .get(path)
            .cloned()
        {
            return cached;
        }

        let normalized = normalize_resolver_dependency_uncached(path);
        let mut cache = self
            .path_cache
            .lock()
            .expect("resolver path cache mutex should not be poisoned");
        cache
            .normalized
            .entry(path.to_path_buf())
            .or_insert_with(|| normalized.clone())
            .clone()
    }

    fn path_is_dir(&self, path: &Path) -> bool {
        if let Some(is_dir) = self
            .path_cache
            .lock()
            .expect("resolver path cache mutex should not be poisoned")
            .is_dir
            .get(path)
            .copied()
        {
            return is_dir;
        }

        let is_dir = path.is_dir();
        self.path_cache
            .lock()
            .expect("resolver path cache mutex should not be poisoned")
            .is_dir
            .entry(path.to_path_buf())
            .or_insert(is_dir);
        is_dir
    }

    fn context_dependencies(
        &self,
        search_directory: &Path,
        file_dependencies: &BTreeSet<PathBuf>,
        missing_dependencies: &BTreeSet<PathBuf>,
    ) -> BTreeSet<PathBuf> {
        let search_directory = self.normalize_resolver_dependency(search_directory);
        file_dependencies
            .iter()
            .chain(missing_dependencies.iter())
            .filter_map(|path| self.context_dependency(&search_directory, path))
            .collect()
    }

    fn context_dependency(&self, search_directory: &Path, dependency: &Path) -> Option<PathBuf> {
        if dependency
            .file_name()
            .is_some_and(|name| name == "node_modules")
        {
            return None;
        }

        let parent = dependency.parent()?;
        if !self.path_is_dir(parent) {
            return None;
        }

        let parent = self.normalize_resolver_dependency(parent);
        (parent.starts_with(search_directory) || has_component(&parent, "node_modules"))
            .then_some(parent)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveResult {
    pub resource: ResolvedResource,
    pub file_dependencies: BTreeSet<PathBuf>,
    pub context_dependencies: BTreeSet<PathBuf>,
    pub missing_dependencies: BTreeSet<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedResource {
    pub path: PathBuf,
    pub query: Option<String>,
    pub fragment: Option<String>,
}

impl From<ResolvedResource> for ModuleIdentity {
    fn from(resource: ResolvedResource) -> Self {
        let mut identity = ModuleIdentity::new(resource.path);
        identity.query = resource.query;
        identity.fragment = resource.fragment;
        identity
    }
}

fn normalize_resolver_dependency_uncached(path: &Path) -> PathBuf {
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical;
    }

    let Some(parent) = path.parent() else {
        return path.to_path_buf();
    };
    let Ok(canonical_parent) = fs::canonicalize(parent) else {
        return path.to_path_buf();
    };
    match path.file_name() {
        Some(file_name) => canonical_parent.join(file_name),
        None => canonical_parent,
    }
}

fn has_component(path: &Path, name: &str) -> bool {
    path.components()
        .any(|component| component.as_os_str() == name)
}
