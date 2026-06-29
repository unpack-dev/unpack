use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{Error, ModuleIdentity, Result};

pub use rspack_resolver::ResolveOptions;

#[derive(Debug, Clone)]
pub struct UnpackResolver {
    inner: Arc<rspack_resolver::Resolver>,
}

impl UnpackResolver {
    pub fn new(options: ResolveOptions) -> Self {
        Self {
            inner: Arc::new(rspack_resolver::Resolver::new(options)),
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
            .map(|path| normalize_resolver_dependency(path.as_path()))
            .collect::<BTreeSet<_>>();
        let context_dependencies = resolver_context_dependencies(&file_dependencies);

        Ok(ResolveResult {
            resource: ResolvedResource {
                path: resolution.path().to_path_buf(),
                query: resolution.query().map(ToOwned::to_owned),
                fragment: resolution.fragment().map(ToOwned::to_owned),
            },
            file_dependencies,
            context_dependencies,
            missing_dependencies: context
                .missing_dependencies
                .into_iter()
                .map(|path| normalize_resolver_dependency(path.as_path()))
                .collect(),
        })
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

fn normalize_resolver_dependency(path: &Path) -> PathBuf {
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

fn resolver_context_dependencies(file_dependencies: &BTreeSet<PathBuf>) -> BTreeSet<PathBuf> {
    file_dependencies
        .iter()
        .filter(|path| path.file_name() == Some(OsStr::new("package.json")))
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .collect()
}
