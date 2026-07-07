use std::{
    collections::HashSet,
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
            .collect::<HashSet<_>>();
        let missing_dependencies = context
            .missing_dependencies
            .into_iter()
            .map(|path| normalize_resolver_dependency(path.as_path()))
            .collect::<HashSet<_>>();
        let context_dependencies = HashSet::new();

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveResult {
    pub resource: ResolvedResource,
    pub file_dependencies: HashSet<PathBuf>,
    pub context_dependencies: HashSet<PathBuf>,
    pub missing_dependencies: HashSet<PathBuf>,
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn resolve_dependencies_do_not_synthesize_contexts_from_files_or_missing_paths()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let src = temp.path().join("src");
        fs::create_dir_all(&src)?;
        fs::write(src.join("dep.js"), "export const value = 1;")?;

        let mut options = ResolveOptions::default();
        options.extensions = vec![".js".to_string()];
        let resolver = UnpackResolver::new(options);

        let result = resolver.resolve_with_dependencies(&src, "./dep").await?;

        assert!(result.resource.path.ends_with("src/dep.js"));
        assert!(
            result
                .file_dependencies
                .contains(&normalize_resolver_dependency(&src.join("dep.js")))
        );
        assert!(result.context_dependencies.is_empty());

        Ok(())
    }
}
