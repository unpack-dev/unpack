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
        CacheIdentifier::from_borrowed_parts([
            self.context.as_os_str().as_encoded_bytes(),
            self.request.as_bytes(),
        ])
    }
}

impl CacheKey for ModuleIdentity {
    fn cache_identifier(&self) -> CacheIdentifier {
        let module_type: &[u8] = match self.module_type {
            crate::ModuleType::JavaScriptAuto => b"javascript/auto",
            crate::ModuleType::Json => b"json",
            crate::ModuleType::Asset => b"asset",
            crate::ModuleType::AssetResource => b"asset/resource",
            crate::ModuleType::AssetInline => b"asset/inline",
            crate::ModuleType::AssetSource => b"asset/source",
        };
        let query = optional_identifier_part(self.query.as_deref());
        let fragment = optional_identifier_part(self.fragment.as_deref());
        let layer = optional_identifier_part(self.layer.as_deref());
        let loader_count = (self.loaders.len() as u64).to_le_bytes();
        CacheIdentifier::from_borrowed_parts(
            [
                module_type,
                self.resource.as_os_str().as_encoded_bytes(),
                &query,
                &fragment,
                &layer,
                &loader_count,
            ]
            .into_iter()
            .chain(self.loaders.iter().map(|loader| loader.as_bytes())),
        )
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

#[cfg(test)]
mod cache_key_tests {
    use super::*;

    #[test]
    fn borrowed_module_identity_parts_preserve_the_persistent_cache_key() {
        let mut identity = ModuleIdentity::new("/project/src/index.js");
        identity.query = Some("?raw".to_string());
        identity.fragment = Some("#value".to_string());
        identity.layer = Some("client".to_string());
        identity.loaders = vec!["/loader.js?options".to_string()];
        for (module_type, type_key) in [
            (
                crate::ModuleType::JavaScriptAuto,
                b"javascript/auto".as_slice(),
            ),
            (crate::ModuleType::Json, b"json".as_slice()),
            (crate::ModuleType::Asset, b"asset".as_slice()),
            (
                crate::ModuleType::AssetResource,
                b"asset/resource".as_slice(),
            ),
            (crate::ModuleType::AssetInline, b"asset/inline".as_slice()),
            (crate::ModuleType::AssetSource, b"asset/source".as_slice()),
        ] {
            identity.module_type = module_type;
            let mut legacy_parts = vec![
                type_key.to_vec(),
                identity.resource.as_os_str().as_encoded_bytes().to_vec(),
                optional_identifier_part(identity.query.as_deref()),
                optional_identifier_part(identity.fragment.as_deref()),
                optional_identifier_part(identity.layer.as_deref()),
                (identity.loaders.len() as u64).to_le_bytes().to_vec(),
            ];
            legacy_parts.extend(
                identity
                    .loaders
                    .iter()
                    .map(|loader| loader.as_bytes().to_vec()),
            );

            assert_eq!(
                identity.cache_identifier(),
                CacheIdentifier::from_parts(legacy_parts)
            );
        }
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

    pub(crate) fn is_valid_sync_with_cache(
        &self,
        file_system_info: &FileSystemInfo,
        strategy: SnapshotStrategy,
        snapshot_cache: &SnapshotCache,
    ) -> bool {
        file_system_info.is_snapshot_valid_sync_with_cache(&self.snapshot, strategy, snapshot_cache)
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

    pub(crate) fn persistent_parts(&self) -> (&ParsedModule, &str, Option<&[u8]>, Option<u64>) {
        (
            self.built_content.parsed(),
            self.built_content.source(),
            self.built_content.binary_source(),
            Some(self.built_content.source_hash()),
        )
    }

    pub(crate) fn from_persistent_parts(
        parsed: ParsedModule,
        source: String,
        binary_source: Option<Vec<u8>>,
        source_hash: u64,
        snapshot: Snapshot,
    ) -> Self {
        Self {
            built_content: Arc::new(BuiltModuleContent::from_persistent_parts_with_binary(
                parsed,
                source,
                binary_source,
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
