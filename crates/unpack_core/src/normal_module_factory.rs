use std::{
    collections::{BTreeSet, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use dashmap::DashMap;
use tokio::sync::OnceCell;

use crate::{
    Dependency, Error, MatchedLoader, ModuleIdentity, ModuleRule, Result, SnapshotStrategy,
    UnpackResolver,
    build_cache::{NormalModuleFactoryCache, ResolveRecord, ResolveRequest},
    snapshot::{FileSystemInfo, SnapshotCache},
};

#[derive(Debug, Clone)]
pub struct NormalModuleFactory {
    resolver: UnpackResolver,
    cache: NormalModuleFactoryCache,
    file_system_info: FileSystemInfo,
    resolve_snapshot_strategy: SnapshotStrategy,
    runtime_factorize_cache: RuntimeFactorizeCache,
    snapshot_cache: SnapshotCache,
    module_rules: Vec<ModuleRule>,
}

// Per-compilation singleflight cache; separate from BuildCache so cache:false
// still coalesces duplicate factory work within one make run.
type RuntimeFactorizeCache = Arc<DashMap<ResolveRequest, Arc<OnceCell<Result<FactorizedModule>>>>>;

impl NormalModuleFactory {
    pub(crate) fn new(
        resolver: UnpackResolver,
        cache: NormalModuleFactoryCache,
        file_system_info: FileSystemInfo,
        resolve_snapshot_strategy: SnapshotStrategy,
        snapshot_cache: SnapshotCache,
    ) -> Self {
        Self {
            resolver,
            cache,
            file_system_info,
            resolve_snapshot_strategy,
            runtime_factorize_cache: Arc::new(DashMap::new()),
            snapshot_cache,
            module_rules: Vec::new(),
        }
    }

    pub(crate) fn with_module_rules(mut self, module_rules: Vec<ModuleRule>) -> Self {
        self.module_rules = module_rules;
        self
    }

    pub async fn factorize(
        &self,
        context: &Path,
        dependency: &Dependency,
    ) -> Result<FactorizedModule> {
        let request = dependency
            .request()
            .expect("module dependency should have a request");
        let resolve_request = ResolveRequest::new(context, request);
        if let Some(record) = self.cache.get(&resolve_request, None) {
            if record
                .is_valid_with_cache(
                    &self.file_system_info,
                    self.resolve_snapshot_strategy,
                    &self.snapshot_cache,
                )
                .await
            {
                return self.apply_module_rules(FactorizedModule::from_resolve_record(&record));
            }
        }

        self.factorize_with_runtime_cache(context, request, resolve_request)
            .await
            .and_then(|factorized| self.apply_module_rules(factorized))
    }

    async fn factorize_with_runtime_cache(
        &self,
        context: &Path,
        request: &str,
        resolve_request: ResolveRequest,
    ) -> Result<FactorizedModule> {
        let cell = self
            .runtime_factorize_cache
            .entry(resolve_request.clone())
            .or_insert_with(|| Arc::new(OnceCell::new()))
            .clone();

        cell.get_or_init(|| async {
            self.factorize_uncached(context, request, resolve_request)
                .await
        })
        .await
        .clone()
    }

    async fn factorize_uncached(
        &self,
        context: &Path,
        request: &str,
        resolve_request: ResolveRequest,
    ) -> Result<FactorizedModule> {
        let resolved = self
            .resolver
            .resolve_with_dependencies(context, request)
            .await?;
        let identity = ModuleIdentity::from(resolved.resource);
        let resource = identity.resource.clone();
        if !self.cache.is_enabled() {
            return Ok(FactorizedModule {
                identity,
                resource,
                file_dependencies: resolved.file_dependencies,
                context_dependencies: resolved.context_dependencies,
                missing_dependencies: resolved.missing_dependencies,
                loader: None,
            });
        }
        let record = ResolveRecord::new_with_cache(
            identity,
            resource,
            resolved
                .file_dependencies
                .into_iter()
                .collect::<BTreeSet<_>>(),
            resolved
                .context_dependencies
                .into_iter()
                .collect::<BTreeSet<_>>(),
            resolved
                .missing_dependencies
                .into_iter()
                .collect::<BTreeSet<_>>(),
            &self.file_system_info,
            self.resolve_snapshot_strategy,
            &self.snapshot_cache,
        )
        .await?;
        let factorized = FactorizedModule::from_resolve_record(&record);
        self.cache.store(resolve_request, None, record);

        Ok(factorized)
    }

    fn apply_module_rules(&self, mut factorized: FactorizedModule) -> Result<FactorizedModule> {
        let mut matching = self
            .module_rules
            .iter()
            .filter(|rule| rule.matches(&factorized.resource));
        let Some(rule) = matching.next() else {
            return Ok(factorized);
        };
        if matching.next().is_some() {
            return Err(Error::LoaderRules {
                path: factorized.resource,
                message: "multiple matching rules would require a loader chain".to_string(),
            });
        }

        let loader = rule.matched_loader();
        factorized.identity.loaders = vec![loader.identifier.clone()];
        factorized.file_dependencies.insert(loader.loader.clone());
        factorized.loader = Some(loader);
        Ok(factorized)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactorizedModule {
    pub identity: ModuleIdentity,
    pub resource: PathBuf,
    pub file_dependencies: HashSet<PathBuf>,
    pub context_dependencies: HashSet<PathBuf>,
    pub missing_dependencies: HashSet<PathBuf>,
    pub loader: Option<MatchedLoader>,
}

impl FactorizedModule {
    fn from_resolve_record(record: &ResolveRecord) -> Self {
        Self {
            identity: record.identity().clone(),
            resource: record.resource().to_path_buf(),
            file_dependencies: record.file_dependencies().iter().cloned().collect(),
            context_dependencies: record.context_dependencies().iter().cloned().collect(),
            missing_dependencies: record.missing_dependencies().iter().cloned().collect(),
            loader: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::{
        CacheOptions, Dependency, DependencyKind, UnpackResolver, build_cache::BuildCache,
        resolver::ResolveOptions,
    };

    #[tokio::test]
    async fn runtime_factorize_cache_reuses_results_when_build_cache_is_disabled()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        fs::write(temp.path().join("dep.js"), "export const value = 1;")?;

        let mut resolve_options = ResolveOptions::default();
        resolve_options.extensions = vec![".js".to_string()];
        let build_cache =
            BuildCache::new(CacheOptions::disabled(), crate::SnapshotOptions::default());
        let factory = NormalModuleFactory::new(
            UnpackResolver::new(resolve_options),
            build_cache.normal_module_factory(),
            FileSystemInfo::new(),
            SnapshotStrategy::timestamp(),
            SnapshotCache::default(),
        );
        let dependency = Dependency::new(DependencyKind::StaticImport, "./dep");

        let first = factory.factorize(temp.path(), &dependency).await?;
        let second = factory.factorize(temp.path(), &dependency).await?;

        assert_eq!(first, second);
        assert_eq!(build_cache.stats().resolve_entries, 0);
        assert_eq!(factory.runtime_factorize_cache.len(), 1);

        Ok(())
    }
}
