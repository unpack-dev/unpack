use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use crate::{
    Dependency, ModuleIdentity, Result, SnapshotStrategy, UnpackResolver,
    build_cache::{NormalModuleFactoryCache, ResolveRecord, ResolveRequest},
};

#[derive(Debug, Clone)]
pub struct NormalModuleFactory {
    resolver: UnpackResolver,
    cache: NormalModuleFactoryCache,
    resolve_snapshot_strategy: SnapshotStrategy,
}

impl NormalModuleFactory {
    pub(crate) fn new(
        resolver: UnpackResolver,
        cache: NormalModuleFactoryCache,
        resolve_snapshot_strategy: SnapshotStrategy,
    ) -> Self {
        Self {
            resolver,
            cache,
            resolve_snapshot_strategy,
        }
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
        if let Some(record) = self.cache.get(&resolve_request) {
            if record.is_valid(self.resolve_snapshot_strategy).await {
                return Ok(FactorizedModule::from_resolve_record(record));
            }
        }

        let resolved = self
            .resolver
            .resolve_with_dependencies(context, request)
            .await?;
        let identity = ModuleIdentity::from(resolved.resource);
        let resource = identity.resource.clone();
        let record = ResolveRecord::new(
            identity,
            resource,
            resolved.file_dependencies,
            resolved.missing_dependencies,
            self.resolve_snapshot_strategy,
        )
        .await?;
        self.cache.store(resolve_request, record.clone());

        Ok(FactorizedModule::from_resolve_record(record))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactorizedModule {
    pub identity: ModuleIdentity,
    pub resource: PathBuf,
    pub file_dependencies: BTreeSet<PathBuf>,
    pub missing_dependencies: BTreeSet<PathBuf>,
}

impl FactorizedModule {
    fn from_resolve_record(record: ResolveRecord) -> Self {
        Self {
            identity: record.identity().clone(),
            resource: record.resource().to_path_buf(),
            file_dependencies: record.file_dependencies().clone(),
            missing_dependencies: record.missing_dependencies().clone(),
        }
    }
}
