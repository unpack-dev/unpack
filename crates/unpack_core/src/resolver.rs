use std::{
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
        let resolution = self
            .inner
            .resolve(directory, request)
            .await
            .map_err(|error| Error::resolve(directory, request, error))?;

        Ok(ResolvedResource {
            path: resolution.path().to_path_buf(),
            query: resolution.query().map(ToOwned::to_owned),
            fragment: resolution.fragment().map(ToOwned::to_owned),
        })
    }
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
