// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/CacheFacade.js

//! Rust-native Cache Items and their stable cache keys.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    ModuleIdentity, SnapshotStrategy,
    module::BuiltModuleContent,
    parser::ParsedModule,
    snapshot::{FileSystemInfo, Snapshot, SnapshotCache},
};

use crate::cache_facade::{CacheIdentifier, CacheKey};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ResolveRequest {
    context: PathBuf,
    request: String,
}

impl ResolveRequest {
    pub(crate) fn new(context: impl Into<PathBuf>, request: impl Into<String>) -> Self {
        Self {
            context: context.into(),
            request: request.into(),
        }
    }
}

impl CacheKey for ResolveRequest {
    fn cache_identifier(&self) -> CacheIdentifier {
        CacheIdentifier::from_parts([
            self.context.as_os_str().as_encoded_bytes().to_vec(),
            self.request.as_bytes().to_vec(),
        ])
    }
}

impl CacheKey for ModuleIdentity {
    fn cache_identifier(&self) -> CacheIdentifier {
        let mut parts = vec![
            match self.module_type {
                crate::ModuleType::JavaScriptAuto => b"javascript/auto".to_vec(),
            },
            self.resource.as_os_str().as_encoded_bytes().to_vec(),
            optional_identifier_part(self.query.as_deref()),
            optional_identifier_part(self.fragment.as_deref()),
            optional_identifier_part(self.layer.as_deref()),
            (self.loaders.len() as u64).to_le_bytes().to_vec(),
        ];
        parts.extend(self.loaders.iter().map(|loader| loader.as_bytes().to_vec()));
        CacheIdentifier::from_parts(parts)
    }
}

fn optional_identifier_part(value: Option<&str>) -> Vec<u8> {
    match value {
        Some(value) => {
            let mut bytes = Vec::with_capacity(value.len() + 1);
            bytes.push(1);
            bytes.extend_from_slice(value.as_bytes());
            bytes
        }
        None => vec![0],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolveRecord {
    identity: ModuleIdentity,
    resource: PathBuf,
    file_dependencies: BTreeSet<PathBuf>,
    context_dependencies: BTreeSet<PathBuf>,
    missing_dependencies: BTreeSet<PathBuf>,
    snapshot: Snapshot,
}

impl ResolveRecord {
    #[cfg(test)]
    pub(crate) async fn new(
        identity: ModuleIdentity,
        resource: PathBuf,
        file_dependencies: BTreeSet<PathBuf>,
        context_dependencies: BTreeSet<PathBuf>,
        missing_dependencies: BTreeSet<PathBuf>,
        file_system_info: &FileSystemInfo,
        strategy: SnapshotStrategy,
    ) -> crate::Result<Self> {
        let snapshot = file_system_info
            .create_resolve_snapshot(
                file_dependencies.iter().cloned(),
                context_dependencies.iter().cloned(),
                missing_dependencies.iter().cloned(),
                strategy,
            )
            .await?;
        Ok(Self {
            identity,
            resource,
            file_dependencies,
            context_dependencies,
            missing_dependencies,
            snapshot,
        })
    }

    pub(crate) async fn new_with_cache(
        identity: ModuleIdentity,
        resource: PathBuf,
        file_dependencies: BTreeSet<PathBuf>,
        context_dependencies: BTreeSet<PathBuf>,
        missing_dependencies: BTreeSet<PathBuf>,
        file_system_info: &FileSystemInfo,
        strategy: SnapshotStrategy,
        snapshot_cache: &SnapshotCache,
    ) -> crate::Result<Self> {
        let snapshot = file_system_info
            .create_resolve_snapshot_with_cache(
                file_dependencies.iter().cloned(),
                context_dependencies.iter().cloned(),
                missing_dependencies.iter().cloned(),
                strategy,
                snapshot_cache,
            )
            .await?;
        Ok(Self {
            identity,
            resource,
            file_dependencies,
            context_dependencies,
            missing_dependencies,
            snapshot,
        })
    }

    pub(crate) fn identity(&self) -> &ModuleIdentity {
        &self.identity
    }

    pub(crate) fn resource(&self) -> &Path {
        &self.resource
    }

    pub(crate) fn file_dependencies(&self) -> &BTreeSet<PathBuf> {
        &self.file_dependencies
    }

    pub(crate) fn context_dependencies(&self) -> &BTreeSet<PathBuf> {
        &self.context_dependencies
    }

    pub(crate) fn missing_dependencies(&self) -> &BTreeSet<PathBuf> {
        &self.missing_dependencies
    }

    pub(crate) fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub(crate) fn from_persistent_parts(
        identity: ModuleIdentity,
        resource: PathBuf,
        file_dependencies: BTreeSet<PathBuf>,
        context_dependencies: BTreeSet<PathBuf>,
        missing_dependencies: BTreeSet<PathBuf>,
        snapshot: Snapshot,
    ) -> Self {
        Self {
            identity,
            resource,
            file_dependencies,
            context_dependencies,
            missing_dependencies,
            snapshot,
        }
    }

    #[cfg(test)]
    pub(crate) async fn is_valid(
        &self,
        file_system_info: &FileSystemInfo,
        strategy: SnapshotStrategy,
    ) -> bool {
        file_system_info
            .is_snapshot_valid(&self.snapshot, strategy)
            .await
    }

    pub(crate) async fn is_valid_with_cache(
        &self,
        file_system_info: &FileSystemInfo,
        strategy: SnapshotStrategy,
        snapshot_cache: &SnapshotCache,
    ) -> bool {
        file_system_info
            .is_snapshot_valid_with_cache(&self.snapshot, strategy, snapshot_cache)
            .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModuleBuildRecord {
    built_content: Arc<BuiltModuleContent>,
    snapshot: Snapshot,
}

impl ModuleBuildRecord {
    pub(crate) fn new(built_content: Arc<BuiltModuleContent>, snapshot: Snapshot) -> Self {
        Self {
            built_content,
            snapshot,
        }
    }

    pub(crate) fn parsed(&self) -> &ParsedModule {
        self.built_content.parsed()
    }

    pub(crate) fn built_content(&self) -> &Arc<BuiltModuleContent> {
        &self.built_content
    }

    pub(crate) fn persistent_parts(&self) -> (&ParsedModule, &str, Option<u64>) {
        (
            self.built_content.parsed(),
            self.built_content.source(),
            Some(self.built_content.source_hash()),
        )
    }

    pub(crate) fn from_persistent_parts(
        parsed: ParsedModule,
        source: String,
        source_hash: u64,
        snapshot: Snapshot,
    ) -> Self {
        Self {
            built_content: Arc::new(BuiltModuleContent::from_persistent_parts(
                parsed,
                source,
                source_hash,
            )),
            snapshot,
        }
    }

    pub(crate) fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub(crate) async fn is_valid_with_cache(
        &self,
        file_system_info: &FileSystemInfo,
        strategy: SnapshotStrategy,
        snapshot_cache: &SnapshotCache,
    ) -> bool {
        file_system_info
            .is_snapshot_valid_with_cache(&self.snapshot, strategy, snapshot_cache)
            .await
    }
}
