// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/cache/PackFileCacheStrategy.js

//! Stable Persistent Cache record DTOs and codecs used by the cache strategy.

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::cache_hash::stable_hash;

    use super::*;

    #[test]
    fn resolve_records_round_trip_through_the_registered_private_codec()
    -> Result<(), Box<dyn std::error::Error>> {
        let codec = ResolveRecordCodec::current();
        let record = resolve_record("dep.js");

        assert_eq!(RESOLVE_RECORD_TYPE_ID.as_bytes(), b"unpack.resolve.1");
        assert_eq!(codec.decode(&codec.encode(&record)?), Some(record));
        Ok(())
    }

    #[test]
    fn module_build_records_round_trip_every_parsed_module_variant()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut json_record = module_build_record();
        json_record.parsed.data = ParsedModuleDataDto::Json(
            r#"{"default":"property","__proto__":{"safe":true}}"#.to_string(),
        );
        let mut asset_record = module_build_record();
        asset_record.parsed.data = ParsedModuleDataDto::Asset {
            module_type: ModuleTypeDto::AssetInline,
        };
        let codec = ModuleBuildRecordCodec::current();

        assert_eq!(MODULE_BUILD_RECORD_TYPE_ID.as_bytes(), b"unpack.moduleb.1");
        assert_eq!(codec.codec_id(), StableCodecId::new(*b"unpack.modb.c003"));
        for record in [module_build_record(), json_record, asset_record] {
            assert_eq!(codec.decode(&codec.encode(&record)?), Some(record));
        }
        Ok(())
    }

    #[test]
    fn asset_render_records_round_trip_through_the_registered_private_codec()
    -> Result<(), Box<dyn std::error::Error>> {
        let record = AssetRenderRecordDto {
            source: "console.log('cached render');\n".to_string(),
            source_map: Some(
                r#"{"version":3,"sources":["src/index.js"],"names":[],"mappings":"AAAA","sourcesContent":["console.log('cached render');\\n"]}"#
                    .to_string(),
            ),
        };
        let codec = AssetRenderRecordCodec::current();

        assert_eq!(ASSET_RENDER_RECORD_TYPE_ID.as_bytes(), b"unpack.asset-r.1");
        assert_eq!(codec.decode(&codec.encode(&record)?), Some(record));
        Ok(())
    }

    #[test]
    fn asset_render_codec_rejects_trailing_bytes_and_invalid_source_maps_on_restore()
    -> Result<(), Box<dyn std::error::Error>> {
        let codec = AssetRenderRecordCodec::current();
        let record = AssetRenderRecordDto {
            source: "rendered".to_string(),
            source_map: None,
        };
        let mut trailing = codec.encode(&record)?;
        trailing.push(0xff);
        assert!(codec.decode(&trailing).is_none());

        assert!(
            RenderedSource::try_from(AssetRenderRecordDto {
                source: "rendered".to_string(),
                source_map: Some("not source map json".to_string()),
            })
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn asset_render_codec_accepts_sources_larger_than_the_generic_field_limit()
    -> Result<(), Box<dyn std::error::Error>> {
        let codec = AssetRenderRecordCodec::current();
        let record = AssetRenderRecordDto {
            source: "x".repeat(MAX_FIELD_BYTES + 1),
            source_map: None,
        };

        let encoded = codec.encode(&record)?;
        assert_eq!(codec.decode(&encoded), Some(record));
        Ok(())
    }

    #[test]
    fn code_generation_records_round_trip_through_the_registered_private_codec()
    -> Result<(), Box<dyn std::error::Error>> {
        let record = CodeGenerationRecordDto {
            source: CodeGenerationSourceDto::OriginalWithReplacements {
                prefix: "(() => {\n".to_string(),
                original_source_len: 28,
                original_name: "./src/index.js".to_string(),
                replacements: vec![CodeGenerationReplacementDto {
                    start: 21,
                    end: 27,
                    content: "after".to_string(),
                    name: None,
                    enforce: ReplacementEnforceDto::Normal,
                }],
                suffix: "\n})".to_string(),
            },
            runtime_requirements: RuntimeRequirements::valid_mask(),
        };
        let codec = CodeGenerationRecordCodec::current();

        assert_eq!(
            CODE_GENERATION_RECORD_TYPE_ID.as_bytes(),
            b"unpack.codegen.1"
        );
        assert_eq!(codec.decode(&codec.encode(&record)?), Some(record));
        Ok(())
    }

    #[test]
    fn code_generation_codec_covers_raw_sources_and_rejects_invalid_recipes()
    -> Result<(), Box<dyn std::error::Error>> {
        let codec = CodeGenerationRecordCodec::current();
        let raw = CodeGenerationRecordDto {
            source: CodeGenerationSourceDto::Raw {
                source: "throw new Error('failed module');".to_string(),
            },
            runtime_requirements: 0,
        };
        assert_eq!(codec.decode(&codec.encode(&raw)?), Some(raw.clone()));

        let invalid_range = CodeGenerationRecordDto {
            source: CodeGenerationSourceDto::OriginalWithReplacements {
                prefix: String::new(),
                original_source_len: 5,
                original_name: "./short.js".to_string(),
                replacements: vec![CodeGenerationReplacementDto {
                    start: 0,
                    end: 6,
                    content: String::new(),
                    name: None,
                    enforce: ReplacementEnforceDto::Normal,
                }],
                suffix: String::new(),
            },
            runtime_requirements: 0,
        };
        assert!(codec.encode(&invalid_range).is_err());

        let mut unknown_tag = codec.encode(&CodeGenerationRecordDto {
            source: CodeGenerationSourceDto::Raw {
                source: "valid".to_string(),
            },
            runtime_requirements: 0,
        })?;
        unknown_tag[4] = 0xff;
        assert!(codec.decode(&unknown_tag).is_none());

        let unknown_requirement = CodeGenerationRecordDto {
            source: CodeGenerationSourceDto::Raw {
                source: "valid".to_string(),
            },
            runtime_requirements: 1 << 15,
        };
        assert!(codec.encode(&unknown_requirement).is_err());
        let mut corrupted_requirement = codec.encode(&raw)?;
        corrupted_requirement[..4].copy_from_slice(&(1_u32 << 15).to_le_bytes());
        assert!(codec.decode(&corrupted_requirement).is_none());
        Ok(())
    }

    #[test]
    fn module_build_codec_rejects_unknown_tags_hash_mismatches_and_invalid_numeric_values()
    -> Result<(), Box<dyn std::error::Error>> {
        let codec = ModuleBuildRecordCodec::current();
        let record = module_build_record();

        let mut unknown_tag = codec.encode(&record)?;
        let first_dependency_tag = 4 + record.source.len() + 8 + 4;
        unknown_tag[first_dependency_tag] = 0xff;
        assert!(codec.decode(&unknown_tag).is_none());

        let mut wrong_hash = record.clone();
        wrong_hash.source_hash ^= 1;
        assert!(codec.encode(&wrong_hash).is_err());

        let mut invalid_range = record.clone();
        if let DependencyDto::Entry { module } = &mut invalid_range.parsed.dependencies[0] {
            module.range = Some(SourceRangeDto { start: 2, end: 1 });
        }
        assert!(codec.encode(&invalid_range).is_err());

        let mut overflow = record;
        if let DependencyDto::Entry { module } = &mut overflow.parsed.dependencies[0] {
            module.source_order = Some(u64::MAX);
        }
        assert!(codec.encode(&overflow).is_err());

        let mut missing_import_range = module_build_record();
        if let DependencyDto::Import { module } = missing_import_range
            .parsed
            .dependencies
            .last_mut()
            .expect("fixture should contain Import")
        {
            module.range = None;
        }
        assert!(codec.encode(&missing_import_range).is_err());
        Ok(())
    }

    #[test]
    fn production_records_convert_to_and_from_their_persistent_dtos()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut resolve_dto = resolve_record("conversion.js");
        if let SnapshotEntryDto::File { modified, .. } = &mut resolve_dto.snapshot.entries[0] {
            *modified = Some(TimestampDto {
                seconds: -1,
                nanoseconds: 999_999_999,
            });
        }
        resolve_dto
            .file_dependencies
            .sort_by(|left, right| left.0.cmp(&right.0));
        let resolve_record = ResolveRecord::try_from(resolve_dto.clone())?;
        assert_eq!(ResolveRecordDto::try_from(&resolve_record)?, resolve_dto);

        let module_dto = module_build_record();
        let module_record = ModuleBuildRecord::try_from(module_dto.clone())?;
        assert_eq!(ModuleBuildRecordDto::try_from(&module_record)?, module_dto);

        let mut runtime_requirements = RuntimeRequirements::default();
        for requirement in RuntimeRequirements::all() {
            runtime_requirements.insert(requirement);
        }
        let code_generation_record =
            CodeGenerationRecord::new(CodeGenerationSource::OriginalWithReplacements {
                prefix: "prefix".to_string(),
                original_source_len: 6,
                original_name: "./fixture.js".to_string(),
                replacements: vec![CodeGenerationReplacement {
                    start: 0,
                    end: 6,
                    content: "after".to_string(),
                    name: None,
                    enforce: ReplacementEnforce::Normal,
                }],
                suffix: "suffix".to_string(),
            })
            .with_runtime_requirements(runtime_requirements);
        let code_generation_dto = CodeGenerationRecordDto::from(&code_generation_record);
        assert_eq!(
            CodeGenerationRecord::try_from(code_generation_dto)?,
            code_generation_record
        );
        Ok(())
    }

    #[test]
    fn resolve_codec_covers_every_snapshot_variant_and_rejects_invalid_timestamps()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut record = resolve_record("variants.js");
        if let SnapshotEntryDto::File { modified, .. } = &mut record.snapshot.entries[0] {
            *modified = Some(TimestampDto {
                seconds: -1,
                nanoseconds: 999_999_999,
            });
        }
        record.snapshot.entries.extend([
            SnapshotEntryDto::ImmutablePath {
                path: PathBytes(vec![b'/', b'p', b'k', b'g', 0xff]),
            },
            SnapshotEntryDto::ManagedPath {
                path: PathBytes::from_path(Path::new("/project/node_modules")),
                root: PathBytes::from_path(Path::new("/project/node_modules")),
                state: ManagedItemStateDto::NodeModules,
            },
            SnapshotEntryDto::ManagedPath {
                path: PathBytes::from_path(Path::new("/project/node_modules/@scope")),
                root: PathBytes::from_path(Path::new("/project/node_modules/@scope")),
                state: ManagedItemStateDto::GroupingFolder,
            },
            SnapshotEntryDto::ManagedPath {
                path: PathBytes::from_path(Path::new("/project/node_modules/pkg/index.js")),
                root: PathBytes::from_path(Path::new("/project/node_modules/pkg")),
                state: ManagedItemStateDto::Package {
                    name: "pkg".to_string(),
                    version: "1.0.0".to_string(),
                },
            },
        ]);
        let codec = ResolveRecordCodec::current();
        assert_eq!(codec.decode(&codec.encode(&record)?), Some(record));

        let mut invalid = resolve_record("invalid-time.js");
        if let SnapshotEntryDto::File { modified, .. } = &mut invalid.snapshot.entries[0] {
            *modified = Some(TimestampDto {
                seconds: 0,
                nanoseconds: 1_000_000_000,
            });
        }
        assert!(codec.encode(&invalid).is_err());

        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn path_dto_preserves_non_utf8_linux_path_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let path = PathBytes(vec![b'/', b'p', b'k', b'g', 0xff]);
        assert_eq!(
            path.clone()
                .into_path_buf()
                .expect("Linux path bytes should be recoverable")
                .as_os_str()
                .as_bytes(),
            path.0
        );
    }

    fn resolve_record(filename: &str) -> ResolveRecordDto {
        let resource = PathBytes::from_path(Path::new(&format!("/project/src/{filename}")));
        ResolveRecordDto {
            identity: ModuleIdentityDto {
                module_type: ModuleTypeDto::JavaScriptAuto,
                resource: resource.clone(),
                query: None,
                fragment: None,
                layer: None,
                loaders: Vec::new(),
            },
            resource: resource.clone(),
            file_dependencies: vec![
                resource.clone(),
                PathBytes::from_path(Path::new("/project/package.json")),
            ],
            context_dependencies: vec![PathBytes::from_path(Path::new("/project/src"))],
            missing_dependencies: vec![PathBytes::from_path(Path::new("/project/src/dep.ts"))],
            snapshot: SnapshotDto {
                entries: vec![
                    SnapshotEntryDto::File {
                        path: resource,
                        exists: true,
                        modified: Some(TimestampDto {
                            seconds: 1_700_000_000,
                            nanoseconds: 123,
                        }),
                        source_hash: Some(42),
                    },
                    SnapshotEntryDto::Context {
                        path: PathBytes::from_path(Path::new("/project/src")),
                        exists: true,
                        timestamp_hash: Some(43),
                        content_hash: Some(44),
                    },
                    SnapshotEntryDto::MissingExistence {
                        path: PathBytes::from_path(Path::new("/project/src/dep.ts")),
                    },
                ],
            },
        }
    }

    fn module_build_record() -> ModuleBuildRecordDto {
        let source =
            "export default 1;                                                              "
                .to_string();
        let module = ModuleDependencyDto {
            request: "./dep".to_string(),
            user_request: "./dep".to_string(),
            source_order: Some(1),
            range: Some(SourceRangeDto { start: 0, end: 1 }),
            weak: false,
        };
        let dependencies = vec![
            DependencyDto::Entry {
                module: module.clone(),
            },
            DependencyDto::HarmonyImportSideEffect {
                module: module.clone(),
                import_var: Some("__dep__".to_string()),
            },
            DependencyDto::HarmonyImportSpecifier {
                module: module.clone(),
                ids: vec!["value".to_string()],
                name: "local".to_string(),
                usage_range: SourceRangeDto { start: 1, end: 2 },
                shorthand: true,
            },
            DependencyDto::HarmonyExportHeader {
                declaration_range: Some(SourceRangeDto { start: 2, end: 3 }),
                statement_range: SourceRangeDto { start: 2, end: 4 },
            },
            DependencyDto::HarmonyExportSpecifier {
                id: "local".to_string(),
                name: "public".to_string(),
            },
            DependencyDto::HarmonyExportExpression {
                range: SourceRangeDto { start: 4, end: 5 },
                statement_range: SourceRangeDto { start: 4, end: 6 },
                declaration_id: Some("default".to_string()),
            },
            DependencyDto::HarmonyExportImportedSpecifier {
                module: module.clone(),
                ids: vec!["value".to_string()],
                name: Some("renamed".to_string()),
                is_star: false,
            },
            DependencyDto::Null,
            DependencyDto::Const {
                expression: "void 0".to_string(),
                range: SourceRangeDto { start: 6, end: 7 },
            },
            DependencyDto::Import {
                module: module.clone(),
            },
        ];
        ModuleBuildRecordDto {
            parsed: ParsedModuleDto {
                dependencies: dependencies.clone(),
                blocks: vec![AsyncDependenciesBlockDto {
                    dependencies: vec![DependencyDto::Import {
                        module: module.clone(),
                    }],
                }],
                presentational_dependencies: dependencies,
                data: ParsedModuleDataDto::JavaScript,
                build_side_effect_free: Some(true),
            },
            source_hash: stable_hash(&source),
            source,
            binary_source: None,
            snapshot: resolve_record("module.js").snapshot,
        }
    }
}
// Stable Persistent Cache record DTOs and codecs.

use std::{
    collections::BTreeSet,
    io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rspack_sources::ReplacementEnforce;

use crate::{
    AsyncDependenciesBlock, ConstDependency, DependenciesBlock, Dependency, EntryDependency,
    HarmonyExportExpressionDependency, HarmonyExportHeaderDependency,
    HarmonyExportImportedSpecifierDependency, HarmonyExportSpecifierDependency,
    HarmonyImportSideEffectDependency, HarmonyImportSpecifierDependency, ImportDependency,
    ModuleDependency, ModuleIdentity, ModuleType, NullDependency, SourceRange,
    cache::{ModuleBuildRecord, ResolveRecord},
    cache_hash::stable_hash,
    code_generation_record::{
        CodeGenerationRecord, CodeGenerationReplacement, CodeGenerationSource,
    },
    parser::ParsedModule,
    rendered_source::RenderedSource,
    runtime::RuntimeRequirements,
    serialization::{
        ItemCodec, MAX_SERIALIZED_ITEM_BYTES, SerializableItem, StableCodecId, StableTypeId,
    },
    snapshot::{PersistentManagedItemState, PersistentSnapshotEntry, Snapshot},
};

#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};

const MAX_RECORD_BYTES: usize = MAX_SERIALIZED_ITEM_BYTES;
const MAX_FIELD_BYTES: usize = 1024 * 1024;
const MAX_COLLECTION_ENTRIES: usize = 100_000;
pub(crate) const DEFAULT_MAX_AGE: Duration = Duration::from_secs(60 * 24 * 60 * 60);
const RESOLVE_RECORD_CODEC_ID: StableCodecId = StableCodecId::new(*b"unpack.rslv.c001");
const MODULE_BUILD_RECORD_CODEC_ID: StableCodecId = StableCodecId::new(*b"unpack.modb.c003");
const CODE_GENERATION_RECORD_CODEC_ID: StableCodecId = StableCodecId::new(*b"unpack.cgen.c003");
const ASSET_RENDER_RECORD_CODEC_ID: StableCodecId = StableCodecId::new(*b"unpack.astr.c001");
pub(crate) const RESOLVE_RECORD_TYPE_ID: StableTypeId = StableTypeId::new(*b"unpack.resolve.1");
pub(crate) const MODULE_BUILD_RECORD_TYPE_ID: StableTypeId =
    StableTypeId::new(*b"unpack.moduleb.1");
pub(crate) const CODE_GENERATION_RECORD_TYPE_ID: StableTypeId =
    StableTypeId::new(*b"unpack.codegen.1");
pub(crate) const ASSET_RENDER_RECORD_TYPE_ID: StableTypeId =
    StableTypeId::new(*b"unpack.asset-r.1");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct AccessStamp {
    pub(super) unix_millis: u64,
}

impl AccessStamp {
    pub(crate) const fn from_millis(unix_millis: u64) -> Self {
        Self { unix_millis }
    }

    pub(crate) fn now() -> Self {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Self::from_millis(u64::try_from(millis).unwrap_or(u64::MAX))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PackFileRetention {
    pub(super) now: AccessStamp,
    pub(super) max_age: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PackFileCompression {
    None,
    Gzip,
    Brotli,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PackFilePublicationOptions {
    pub(super) retention: PackFileRetention,
    pub(super) compression: PackFileCompression,
}

impl PackFilePublicationOptions {
    pub(crate) const fn new(
        retention: PackFileRetention,
        compression: PackFileCompression,
    ) -> Self {
        Self {
            retention,
            compression,
        }
    }
}

impl PackFileRetention {
    pub(crate) const fn new(now: AccessStamp, max_age: Duration) -> Self {
        Self { now, max_age }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct PackFileAddress {
    pub(super) namespace: Vec<u8>,
    pub(super) identifier: Vec<u8>,
}

impl PackFileAddress {
    pub(crate) fn new(namespace: impl AsRef<[u8]>, identifier: impl AsRef<[u8]>) -> Self {
        Self {
            namespace: namespace.as_ref().to_vec(),
            identifier: identifier.as_ref().to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackFileETag(pub(super) Vec<u8>);

impl PackFileETag {
    pub(crate) fn new(value: impl AsRef<[u8]>) -> Self {
        Self(value.as_ref().to_vec())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PathBytes(Vec<u8>);

impl PathBytes {
    #[cfg(unix)]
    pub(crate) fn from_path(path: &Path) -> Self {
        Self(path.as_os_str().as_bytes().to_vec())
    }

    #[cfg(not(unix))]
    pub(crate) fn from_path(path: &Path) -> Self {
        Self(path.to_string_lossy().as_bytes().to_vec())
    }

    #[cfg(unix)]
    pub(crate) fn into_path_buf(self) -> Option<PathBuf> {
        Some(PathBuf::from(std::ffi::OsString::from_vec(self.0)))
    }

    #[cfg(not(unix))]
    pub(crate) fn into_path_buf(self) -> Option<PathBuf> {
        String::from_utf8(self.0).ok().map(PathBuf::from)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolveRecordDto {
    pub(crate) identity: ModuleIdentityDto,
    pub(crate) resource: PathBytes,
    pub(crate) file_dependencies: Vec<PathBytes>,
    pub(crate) context_dependencies: Vec<PathBytes>,
    pub(crate) missing_dependencies: Vec<PathBytes>,
    pub(crate) snapshot: SnapshotDto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModuleIdentityDto {
    pub(crate) module_type: ModuleTypeDto,
    pub(crate) resource: PathBytes,
    pub(crate) query: Option<String>,
    pub(crate) fragment: Option<String>,
    pub(crate) layer: Option<String>,
    pub(crate) loaders: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModuleTypeDto {
    JavaScriptAuto,
    Json,
    Asset,
    AssetResource,
    AssetInline,
    AssetSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SnapshotDto {
    pub(crate) entries: Vec<SnapshotEntryDto>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SnapshotEntryDto {
    File {
        path: PathBytes,
        exists: bool,
        modified: Option<TimestampDto>,
        source_hash: Option<u64>,
    },
    Context {
        path: PathBytes,
        exists: bool,
        timestamp_hash: Option<u64>,
        content_hash: Option<u64>,
    },
    MissingExistence {
        path: PathBytes,
    },
    ImmutablePath {
        path: PathBytes,
    },
    ManagedPath {
        path: PathBytes,
        root: PathBytes,
        state: ManagedItemStateDto,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimestampDto {
    pub(crate) seconds: i64,
    pub(crate) nanoseconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ManagedItemStateDto {
    NodeModules,
    GroupingFolder,
    Package { name: String, version: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModuleBuildRecordDto {
    pub(crate) parsed: ParsedModuleDto,
    pub(crate) source: String,
    pub(crate) binary_source: Option<Vec<u8>>,
    pub(crate) source_hash: u64,
    pub(crate) snapshot: SnapshotDto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssetRenderRecordDto {
    pub(crate) source: String,
    pub(crate) source_map: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeGenerationRecordDto {
    pub(crate) source: CodeGenerationSourceDto,
    pub(crate) runtime_requirements: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CodeGenerationSourceDto {
    Raw {
        source: String,
    },
    OriginalWithReplacements {
        prefix: String,
        original_source_len: u32,
        original_name: String,
        replacements: Vec<CodeGenerationReplacementDto>,
        suffix: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeGenerationReplacementDto {
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) content: String,
    pub(crate) name: Option<String>,
    pub(crate) enforce: ReplacementEnforceDto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplacementEnforceDto {
    Pre,
    Normal,
    Post,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedModuleDto {
    pub(crate) dependencies: Vec<DependencyDto>,
    pub(crate) blocks: Vec<AsyncDependenciesBlockDto>,
    pub(crate) presentational_dependencies: Vec<DependencyDto>,
    pub(crate) data: ParsedModuleDataDto,
    pub(crate) build_side_effect_free: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParsedModuleDataDto {
    JavaScript,
    Json(String),
    Asset { module_type: ModuleTypeDto },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AsyncDependenciesBlockDto {
    pub(crate) dependencies: Vec<DependencyDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceRangeDto {
    pub(crate) start: u32,
    pub(crate) end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModuleDependencyDto {
    pub(crate) request: String,
    pub(crate) user_request: String,
    pub(crate) source_order: Option<u64>,
    pub(crate) range: Option<SourceRangeDto>,
    pub(crate) weak: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DependencyDto {
    Entry {
        module: ModuleDependencyDto,
    },
    HarmonyImportSideEffect {
        module: ModuleDependencyDto,
        import_var: Option<String>,
    },
    HarmonyImportSpecifier {
        module: ModuleDependencyDto,
        ids: Vec<String>,
        name: String,
        usage_range: SourceRangeDto,
        shorthand: bool,
    },
    HarmonyExportHeader {
        declaration_range: Option<SourceRangeDto>,
        statement_range: SourceRangeDto,
    },
    HarmonyExportSpecifier {
        id: String,
        name: String,
    },
    HarmonyExportExpression {
        range: SourceRangeDto,
        statement_range: SourceRangeDto,
        declaration_id: Option<String>,
    },
    HarmonyExportImportedSpecifier {
        module: ModuleDependencyDto,
        ids: Vec<String>,
        name: Option<String>,
        is_star: bool,
    },
    Null,
    Const {
        expression: String,
        range: SourceRangeDto,
    },
    Import {
        module: ModuleDependencyDto,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackFileGuardDto {
    pub(crate) version: Vec<u8>,
    pub(crate) build_dependencies: SnapshotDto,
    pub(crate) resolve_build_dependencies: SnapshotDto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublicationBase {
    PreserveEntries { expected_revision: u64 },
    ReplaceAll,
}

impl From<&ModuleIdentity> for ModuleIdentityDto {
    fn from(identity: &ModuleIdentity) -> Self {
        Self {
            module_type: match identity.module_type {
                ModuleType::JavaScriptAuto => ModuleTypeDto::JavaScriptAuto,
                ModuleType::Json => ModuleTypeDto::Json,
                ModuleType::Asset => ModuleTypeDto::Asset,
                ModuleType::AssetResource => ModuleTypeDto::AssetResource,
                ModuleType::AssetInline => ModuleTypeDto::AssetInline,
                ModuleType::AssetSource => ModuleTypeDto::AssetSource,
            },
            resource: PathBytes::from_path(&identity.resource),
            query: identity.query.clone(),
            fragment: identity.fragment.clone(),
            layer: identity.layer.clone(),
            loaders: identity.loaders.clone(),
        }
    }
}

impl TryFrom<ModuleIdentityDto> for ModuleIdentity {
    type Error = io::Error;

    fn try_from(identity: ModuleIdentityDto) -> io::Result<Self> {
        Ok(Self {
            module_type: match identity.module_type {
                ModuleTypeDto::JavaScriptAuto => ModuleType::JavaScriptAuto,
                ModuleTypeDto::Json => ModuleType::Json,
                ModuleTypeDto::Asset => ModuleType::Asset,
                ModuleTypeDto::AssetResource => ModuleType::AssetResource,
                ModuleTypeDto::AssetInline => ModuleType::AssetInline,
                ModuleTypeDto::AssetSource => ModuleType::AssetSource,
            },
            resource: path_from_bytes(identity.resource)?,
            query: identity.query,
            fragment: identity.fragment,
            layer: identity.layer,
            loaders: identity.loaders,
        })
    }
}

impl TryFrom<&Snapshot> for SnapshotDto {
    type Error = io::Error;

    fn try_from(snapshot: &Snapshot) -> io::Result<Self> {
        let entries = snapshot
            .persistent_entries()
            .into_iter()
            .map(|entry| {
                Ok(match entry {
                    PersistentSnapshotEntry::File {
                        path,
                        exists,
                        modified,
                        source_hash,
                    } => SnapshotEntryDto::File {
                        path: PathBytes::from_path(&path),
                        exists,
                        modified: modified.map(timestamp_from_system_time).transpose()?,
                        source_hash,
                    },
                    PersistentSnapshotEntry::Context {
                        path,
                        exists,
                        timestamp_hash,
                        content_hash,
                    } => SnapshotEntryDto::Context {
                        path: PathBytes::from_path(&path),
                        exists,
                        timestamp_hash,
                        content_hash,
                    },
                    PersistentSnapshotEntry::MissingExistence { path } => {
                        SnapshotEntryDto::MissingExistence {
                            path: PathBytes::from_path(&path),
                        }
                    }
                    PersistentSnapshotEntry::ImmutablePath { path } => {
                        SnapshotEntryDto::ImmutablePath {
                            path: PathBytes::from_path(&path),
                        }
                    }
                    PersistentSnapshotEntry::ManagedPath { path, root, state } => {
                        SnapshotEntryDto::ManagedPath {
                            path: PathBytes::from_path(&path),
                            root: PathBytes::from_path(&root),
                            state: match state {
                                PersistentManagedItemState::NodeModules => {
                                    ManagedItemStateDto::NodeModules
                                }
                                PersistentManagedItemState::GroupingFolder => {
                                    ManagedItemStateDto::GroupingFolder
                                }
                                PersistentManagedItemState::Package { name, version } => {
                                    ManagedItemStateDto::Package { name, version }
                                }
                            },
                        }
                    }
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self { entries })
    }
}

impl TryFrom<SnapshotDto> for Snapshot {
    type Error = io::Error;

    fn try_from(snapshot: SnapshotDto) -> io::Result<Self> {
        let entries = snapshot
            .entries
            .into_iter()
            .map(|entry| {
                Ok(match entry {
                    SnapshotEntryDto::File {
                        path,
                        exists,
                        modified,
                        source_hash,
                    } => PersistentSnapshotEntry::File {
                        path: path_from_bytes(path)?,
                        exists,
                        modified: modified.map(system_time_from_timestamp).transpose()?,
                        source_hash,
                    },
                    SnapshotEntryDto::Context {
                        path,
                        exists,
                        timestamp_hash,
                        content_hash,
                    } => PersistentSnapshotEntry::Context {
                        path: path_from_bytes(path)?,
                        exists,
                        timestamp_hash,
                        content_hash,
                    },
                    SnapshotEntryDto::MissingExistence { path } => {
                        PersistentSnapshotEntry::MissingExistence {
                            path: path_from_bytes(path)?,
                        }
                    }
                    SnapshotEntryDto::ImmutablePath { path } => {
                        PersistentSnapshotEntry::ImmutablePath {
                            path: path_from_bytes(path)?,
                        }
                    }
                    SnapshotEntryDto::ManagedPath { path, root, state } => {
                        PersistentSnapshotEntry::ManagedPath {
                            path: path_from_bytes(path)?,
                            root: path_from_bytes(root)?,
                            state: match state {
                                ManagedItemStateDto::NodeModules => {
                                    PersistentManagedItemState::NodeModules
                                }
                                ManagedItemStateDto::GroupingFolder => {
                                    PersistentManagedItemState::GroupingFolder
                                }
                                ManagedItemStateDto::Package { name, version } => {
                                    PersistentManagedItemState::Package { name, version }
                                }
                            },
                        }
                    }
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        Snapshot::from_persistent_entries(entries).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "PackFile Snapshot contains inconsistent or duplicate entries",
            )
        })
    }
}

impl TryFrom<&ResolveRecord> for ResolveRecordDto {
    type Error = io::Error;

    fn try_from(record: &ResolveRecord) -> io::Result<Self> {
        Ok(Self {
            identity: ModuleIdentityDto::from(record.identity()),
            resource: PathBytes::from_path(record.resource()),
            file_dependencies: record
                .file_dependencies()
                .iter()
                .map(|path| PathBytes::from_path(path))
                .collect(),
            context_dependencies: record
                .context_dependencies()
                .iter()
                .map(|path| PathBytes::from_path(path))
                .collect(),
            missing_dependencies: record
                .missing_dependencies()
                .iter()
                .map(|path| PathBytes::from_path(path))
                .collect(),
            snapshot: SnapshotDto::try_from(record.snapshot())?,
        })
    }
}

impl TryFrom<ResolveRecordDto> for ResolveRecord {
    type Error = io::Error;

    fn try_from(record: ResolveRecordDto) -> io::Result<Self> {
        Ok(ResolveRecord::from_persistent_parts(
            ModuleIdentity::try_from(record.identity)?,
            path_from_bytes(record.resource)?,
            paths_from_bytes(record.file_dependencies)?,
            paths_from_bytes(record.context_dependencies)?,
            paths_from_bytes(record.missing_dependencies)?,
            Snapshot::try_from(record.snapshot)?,
        ))
    }
}

impl TryFrom<&ModuleBuildRecord> for ModuleBuildRecordDto {
    type Error = io::Error;

    fn try_from(record: &ModuleBuildRecord) -> io::Result<Self> {
        let (parsed, source, binary_source, source_hash) = record.persistent_parts();
        let source_hash = source_hash.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Module Build Record is missing its required source hash",
            )
        })?;
        let dto = Self {
            parsed: ParsedModuleDto::try_from(parsed)?,
            source: source.to_string(),
            binary_source: binary_source.map(<[u8]>::to_vec),
            source_hash,
            snapshot: SnapshotDto::try_from(record.snapshot())?,
        };
        validate_module_build_record(&dto)?;
        Ok(dto)
    }
}

impl TryFrom<ModuleBuildRecordDto> for ModuleBuildRecord {
    type Error = io::Error;

    fn try_from(record: ModuleBuildRecordDto) -> io::Result<Self> {
        let ModuleBuildRecordDto {
            parsed,
            source,
            binary_source,
            source_hash,
            snapshot,
        } = record;
        Ok(ModuleBuildRecord::from_persistent_parts(
            ParsedModule::try_from(parsed)?,
            source,
            binary_source,
            source_hash,
            Snapshot::try_from(snapshot)?,
        ))
    }
}

impl From<&CodeGenerationRecord> for CodeGenerationRecordDto {
    fn from(record: &CodeGenerationRecord) -> Self {
        let source = match record.source() {
            CodeGenerationSource::Raw { source } => CodeGenerationSourceDto::Raw {
                source: source.clone(),
            },
            CodeGenerationSource::OriginalWithReplacements {
                prefix,
                original_source_len,
                original_name,
                replacements,
                suffix,
            } => CodeGenerationSourceDto::OriginalWithReplacements {
                prefix: prefix.clone(),
                original_source_len: *original_source_len,
                original_name: original_name.clone(),
                replacements: replacements
                    .iter()
                    .map(|replacement| CodeGenerationReplacementDto {
                        start: replacement.start,
                        end: replacement.end,
                        content: replacement.content.clone(),
                        name: replacement.name.clone(),
                        enforce: match replacement.enforce {
                            ReplacementEnforce::Pre => ReplacementEnforceDto::Pre,
                            ReplacementEnforce::Normal => ReplacementEnforceDto::Normal,
                            ReplacementEnforce::Post => ReplacementEnforceDto::Post,
                        },
                    })
                    .collect(),
                suffix: suffix.clone(),
            },
        };
        Self {
            source,
            runtime_requirements: encode_runtime_requirements(record.runtime_requirements()),
        }
    }
}

impl TryFrom<CodeGenerationRecordDto> for CodeGenerationRecord {
    type Error = io::Error;

    fn try_from(record: CodeGenerationRecordDto) -> io::Result<Self> {
        validate_code_generation_record(&record)?;
        Ok(record.into_record_after_codec_validation())
    }
}

impl CodeGenerationRecordDto {
    pub(crate) fn into_record_after_codec_validation(self) -> CodeGenerationRecord {
        let runtime_requirements = decode_runtime_requirements(self.runtime_requirements)
            .expect("Code Generation codec must reject unknown Runtime Requirements");
        let source = match self.source {
            CodeGenerationSourceDto::Raw { source } => CodeGenerationSource::Raw { source },
            CodeGenerationSourceDto::OriginalWithReplacements {
                prefix,
                original_source_len,
                original_name,
                replacements,
                suffix,
            } => CodeGenerationSource::OriginalWithReplacements {
                prefix,
                original_source_len,
                original_name,
                replacements: replacements
                    .into_iter()
                    .map(|replacement| CodeGenerationReplacement {
                        start: replacement.start,
                        end: replacement.end,
                        content: replacement.content,
                        name: replacement.name,
                        enforce: match replacement.enforce {
                            ReplacementEnforceDto::Pre => ReplacementEnforce::Pre,
                            ReplacementEnforceDto::Normal => ReplacementEnforce::Normal,
                            ReplacementEnforceDto::Post => ReplacementEnforce::Post,
                        },
                    })
                    .collect(),
                suffix,
            },
        };
        CodeGenerationRecord::new(source).with_runtime_requirements(runtime_requirements)
    }
}

fn encode_runtime_requirements(requirements: &RuntimeRequirements) -> u16 {
    requirements.to_mask()
}

fn decode_runtime_requirements(mask: u16) -> Option<RuntimeRequirements> {
    RuntimeRequirements::from_mask(mask)
}

impl From<&RenderedSource> for AssetRenderRecordDto {
    fn from(record: &RenderedSource) -> Self {
        let (source, source_map) = record.persistent_parts();
        Self { source, source_map }
    }
}

impl TryFrom<AssetRenderRecordDto> for RenderedSource {
    type Error = io::Error;

    fn try_from(record: AssetRenderRecordDto) -> io::Result<Self> {
        RenderedSource::from_persistent_parts(record.source, record.source_map).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Asset Render Record contains an invalid source map",
            )
        })
    }
}

fn validate_module_build_record(record: &ModuleBuildRecordDto) -> io::Result<()> {
    let source_hash = record
        .binary_source
        .as_ref()
        .map(stable_hash)
        .unwrap_or_else(|| stable_hash(&record.source));
    if record.source_hash != source_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Module Build Record source hash does not match its source",
        ));
    }
    if !parsed_module_ranges_are_valid(&record.parsed, &record.source) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Module Build Record contains an invalid source range",
        ));
    }
    Ok(())
}

fn path_from_bytes(path: PathBytes) -> io::Result<PathBuf> {
    path.into_path_buf().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "PackFile path bytes are invalid on this platform",
        )
    })
}

fn paths_from_bytes(paths: Vec<PathBytes>) -> io::Result<BTreeSet<PathBuf>> {
    paths
        .into_iter()
        .map(path_from_bytes)
        .collect::<io::Result<BTreeSet<_>>>()
}

fn timestamp_from_system_time(time: SystemTime) -> io::Result<TimestampDto> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => Ok(TimestampDto {
            seconds: i64::try_from(duration.as_secs()).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Snapshot timestamp is too large",
                )
            })?,
            nanoseconds: duration.subsec_nanos(),
        }),
        Err(error) => {
            let duration = error.duration();
            let seconds = i64::try_from(duration.as_secs()).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Snapshot timestamp is too small",
                )
            })?;
            if duration.subsec_nanos() == 0 {
                Ok(TimestampDto {
                    seconds: seconds.checked_neg().ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Snapshot timestamp is too small",
                        )
                    })?,
                    nanoseconds: 0,
                })
            } else {
                Ok(TimestampDto {
                    seconds: seconds
                        .checked_neg()
                        .and_then(|seconds| seconds.checked_sub(1))
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Snapshot timestamp is too small",
                            )
                        })?,
                    nanoseconds: 1_000_000_000 - duration.subsec_nanos(),
                })
            }
        }
    }
}

fn system_time_from_timestamp(timestamp: TimestampDto) -> io::Result<SystemTime> {
    if timestamp.nanoseconds >= 1_000_000_000 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Snapshot timestamp nanoseconds are out of range",
        ));
    }
    if timestamp.seconds >= 0 {
        return UNIX_EPOCH
            .checked_add(Duration::new(
                timestamp.seconds.unsigned_abs(),
                timestamp.nanoseconds,
            ))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Snapshot timestamp is too large",
                )
            });
    }

    let (seconds, nanoseconds) = if timestamp.nanoseconds == 0 {
        (timestamp.seconds.unsigned_abs(), 0)
    } else {
        (
            timestamp
                .seconds
                .unsigned_abs()
                .checked_sub(1)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "Snapshot timestamp is invalid")
                })?,
            1_000_000_000 - timestamp.nanoseconds,
        )
    };
    UNIX_EPOCH
        .checked_sub(Duration::new(seconds, nanoseconds))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Snapshot timestamp is too small",
            )
        })
}

impl TryFrom<&ParsedModule> for ParsedModuleDto {
    type Error = io::Error;

    fn try_from(parsed: &ParsedModule) -> io::Result<Self> {
        Ok(Self {
            dependencies: parsed
                .dependencies_block
                .dependencies()
                .iter()
                .map(dependency_to_dto)
                .collect::<io::Result<_>>()?,
            blocks: parsed
                .dependencies_block
                .blocks()
                .iter()
                .map(|block| {
                    Ok(AsyncDependenciesBlockDto {
                        dependencies: block
                            .dependencies()
                            .iter()
                            .map(dependency_to_dto)
                            .collect::<io::Result<_>>()?,
                    })
                })
                .collect::<io::Result<_>>()?,
            presentational_dependencies: parsed
                .presentational_dependencies
                .iter()
                .map(dependency_to_dto)
                .collect::<io::Result<_>>()?,
            data: match &parsed.data {
                crate::parser::ParsedModuleData::JavaScript => ParsedModuleDataDto::JavaScript,
                crate::parser::ParsedModuleData::Json(value) => ParsedModuleDataDto::Json(
                    serde_json::to_string(value).map_err(io::Error::other)?,
                ),
                crate::parser::ParsedModuleData::Asset { module_type } => {
                    ParsedModuleDataDto::Asset {
                        module_type: match module_type {
                            ModuleType::JavaScriptAuto => ModuleTypeDto::JavaScriptAuto,
                            ModuleType::Json => ModuleTypeDto::Json,
                            ModuleType::Asset => ModuleTypeDto::Asset,
                            ModuleType::AssetResource => ModuleTypeDto::AssetResource,
                            ModuleType::AssetInline => ModuleTypeDto::AssetInline,
                            ModuleType::AssetSource => ModuleTypeDto::AssetSource,
                        },
                    }
                }
            },
            build_side_effect_free: parsed.build_meta.side_effect_free,
        })
    }
}

impl TryFrom<ParsedModuleDto> for ParsedModule {
    type Error = io::Error;

    fn try_from(parsed: ParsedModuleDto) -> io::Result<Self> {
        let ParsedModuleDto {
            dependencies,
            blocks,
            presentational_dependencies,
            data,
            build_side_effect_free,
        } = parsed;
        let dependencies = dependencies
            .into_iter()
            .map(dependency_from_dto)
            .collect::<io::Result<_>>()?;
        let blocks = blocks
            .into_iter()
            .map(|block| {
                Ok(AsyncDependenciesBlock::new(
                    block
                        .dependencies
                        .into_iter()
                        .map(dependency_from_dto)
                        .collect::<io::Result<_>>()?,
                ))
            })
            .collect::<io::Result<_>>()?;
        Ok(Self {
            dependencies_block: DependenciesBlock::new(dependencies, blocks),
            presentational_dependencies: presentational_dependencies
                .into_iter()
                .map(dependency_from_dto)
                .collect::<io::Result<_>>()?,
            data: match data {
                ParsedModuleDataDto::JavaScript => crate::parser::ParsedModuleData::JavaScript,
                ParsedModuleDataDto::Json(value) => crate::parser::ParsedModuleData::Json(
                    serde_json::from_str(&value).map_err(io::Error::other)?,
                ),
                ParsedModuleDataDto::Asset { module_type } => {
                    let module_type = match module_type {
                        ModuleTypeDto::Asset => ModuleType::Asset,
                        ModuleTypeDto::AssetResource => ModuleType::AssetResource,
                        ModuleTypeDto::AssetInline => ModuleType::AssetInline,
                        ModuleTypeDto::AssetSource => ModuleType::AssetSource,
                        ModuleTypeDto::JavaScriptAuto | ModuleTypeDto::Json => {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Asset Parser data contains a non-asset module type",
                            ));
                        }
                    };
                    crate::parser::ParsedModuleData::Asset { module_type }
                }
            },
            build_meta: crate::parser::JavascriptBuildMeta {
                side_effect_free: build_side_effect_free,
            },
        })
    }
}

fn dependency_to_dto(dependency: &Dependency) -> io::Result<DependencyDto> {
    Ok(match dependency {
        Dependency::Entry(dependency) => DependencyDto::Entry {
            module: module_dependency_to_dto(&dependency.module)?,
        },
        Dependency::HarmonyImportSideEffect(dependency) => DependencyDto::HarmonyImportSideEffect {
            module: module_dependency_to_dto(&dependency.module)?,
            import_var: dependency.import_var.clone(),
        },
        Dependency::HarmonyImportSpecifier(dependency) => DependencyDto::HarmonyImportSpecifier {
            module: module_dependency_to_dto(&dependency.module)?,
            ids: dependency.ids.clone(),
            name: dependency.name.clone(),
            usage_range: dependency.usage_range.into(),
            shorthand: dependency.shorthand,
        },
        Dependency::HarmonyExportHeader(dependency) => DependencyDto::HarmonyExportHeader {
            declaration_range: dependency.declaration_range.map(Into::into),
            statement_range: dependency.statement_range.into(),
        },
        Dependency::HarmonyExportSpecifier(dependency) => DependencyDto::HarmonyExportSpecifier {
            id: dependency.id.clone(),
            name: dependency.name.clone(),
        },
        Dependency::HarmonyExportExpression(dependency) => DependencyDto::HarmonyExportExpression {
            range: dependency.range.into(),
            statement_range: dependency.statement_range.into(),
            declaration_id: dependency.declaration_id.clone(),
        },
        Dependency::HarmonyExportImportedSpecifier(dependency) => {
            DependencyDto::HarmonyExportImportedSpecifier {
                module: module_dependency_to_dto(&dependency.module)?,
                ids: dependency.ids.clone(),
                name: dependency.name.clone(),
                is_star: dependency.is_star,
            }
        }
        Dependency::Null(_) => DependencyDto::Null,
        Dependency::Const(dependency) => DependencyDto::Const {
            expression: dependency.expression.clone(),
            range: dependency.range.into(),
        },
        Dependency::Import(dependency) => DependencyDto::Import {
            module: module_dependency_to_dto(&dependency.module)?,
        },
    })
}

fn dependency_from_dto(dependency: DependencyDto) -> io::Result<Dependency> {
    Ok(match dependency {
        DependencyDto::Entry { module } => Dependency::Entry(EntryDependency {
            module: module_dependency_from_dto(module)?,
        }),
        DependencyDto::HarmonyImportSideEffect { module, import_var } => {
            Dependency::HarmonyImportSideEffect(HarmonyImportSideEffectDependency {
                module: module_dependency_from_dto(module)?,
                import_var,
            })
        }
        DependencyDto::HarmonyImportSpecifier {
            module,
            ids,
            name,
            usage_range,
            shorthand,
        } => Dependency::HarmonyImportSpecifier(HarmonyImportSpecifierDependency {
            module: module_dependency_from_dto(module)?,
            ids,
            name,
            usage_range: usage_range.into(),
            shorthand,
        }),
        DependencyDto::HarmonyExportHeader {
            declaration_range,
            statement_range,
        } => Dependency::HarmonyExportHeader(HarmonyExportHeaderDependency {
            declaration_range: declaration_range.map(Into::into),
            statement_range: statement_range.into(),
        }),
        DependencyDto::HarmonyExportSpecifier { id, name } => {
            Dependency::HarmonyExportSpecifier(HarmonyExportSpecifierDependency { id, name })
        }
        DependencyDto::HarmonyExportExpression {
            range,
            statement_range,
            declaration_id,
        } => Dependency::HarmonyExportExpression(HarmonyExportExpressionDependency {
            range: range.into(),
            statement_range: statement_range.into(),
            declaration_id,
        }),
        DependencyDto::HarmonyExportImportedSpecifier {
            module,
            ids,
            name,
            is_star,
        } => Dependency::HarmonyExportImportedSpecifier(HarmonyExportImportedSpecifierDependency {
            module: module_dependency_from_dto(module)?,
            ids,
            name,
            is_star,
        }),
        DependencyDto::Null => Dependency::Null(NullDependency),
        DependencyDto::Const { expression, range } => Dependency::Const(ConstDependency {
            expression,
            range: range.into(),
        }),
        DependencyDto::Import { module } => Dependency::Import(ImportDependency {
            module: module_dependency_from_dto(module)?,
        }),
    })
}

fn module_dependency_to_dto(dependency: &ModuleDependency) -> io::Result<ModuleDependencyDto> {
    Ok(ModuleDependencyDto {
        request: dependency.request.clone(),
        user_request: dependency.user_request.clone(),
        source_order: dependency
            .source_order
            .map(u64::try_from)
            .transpose()
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Module dependency source order is too large",
                )
            })?,
        range: dependency.range.map(Into::into),
        weak: dependency.weak,
    })
}

fn module_dependency_from_dto(dependency: ModuleDependencyDto) -> io::Result<ModuleDependency> {
    Ok(ModuleDependency {
        request: dependency.request,
        user_request: dependency.user_request,
        source_order: dependency
            .source_order
            .map(usize::try_from)
            .transpose()
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Module dependency source order overflows this platform",
                )
            })?,
        range: dependency.range.map(Into::into),
        weak: dependency.weak,
    })
}

impl From<SourceRange> for SourceRangeDto {
    fn from(range: SourceRange) -> Self {
        Self {
            start: range.start,
            end: range.end,
        }
    }
}

impl From<SourceRangeDto> for SourceRange {
    fn from(range: SourceRangeDto) -> Self {
        Self::new(range.start, range.end)
    }
}

impl SerializableItem for ResolveRecordDto {
    const TYPE_ID: StableTypeId = RESOLVE_RECORD_TYPE_ID;
}

impl SerializableItem for ModuleBuildRecordDto {
    const TYPE_ID: StableTypeId = MODULE_BUILD_RECORD_TYPE_ID;
}

impl SerializableItem for CodeGenerationRecordDto {
    const TYPE_ID: StableTypeId = CODE_GENERATION_RECORD_TYPE_ID;
}

impl SerializableItem for AssetRenderRecordDto {
    const TYPE_ID: StableTypeId = ASSET_RENDER_RECORD_TYPE_ID;
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ResolveRecordCodec {
    codec_id: StableCodecId,
}

impl ResolveRecordCodec {
    pub(crate) const fn current() -> Self {
        Self {
            codec_id: RESOLVE_RECORD_CODEC_ID,
        }
    }
}

impl ItemCodec<ResolveRecordDto> for ResolveRecordCodec {
    fn codec_id(&self) -> StableCodecId {
        self.codec_id
    }

    fn encode(&self, value: &ResolveRecordDto) -> io::Result<Vec<u8>> {
        encode_resolve_record(value)
    }

    fn decode(&self, bytes: &[u8]) -> Option<ResolveRecordDto> {
        decode_resolve_record(bytes)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ModuleBuildRecordCodec {
    codec_id: StableCodecId,
}

impl ModuleBuildRecordCodec {
    pub(crate) const fn current() -> Self {
        Self {
            codec_id: MODULE_BUILD_RECORD_CODEC_ID,
        }
    }
}

impl ItemCodec<ModuleBuildRecordDto> for ModuleBuildRecordCodec {
    fn codec_id(&self) -> StableCodecId {
        self.codec_id
    }

    fn encode(&self, value: &ModuleBuildRecordDto) -> io::Result<Vec<u8>> {
        encode_module_build_record(value)
    }

    fn decode(&self, bytes: &[u8]) -> Option<ModuleBuildRecordDto> {
        decode_module_build_record(bytes)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CodeGenerationRecordCodec {
    codec_id: StableCodecId,
}

impl CodeGenerationRecordCodec {
    pub(crate) const fn current() -> Self {
        Self {
            codec_id: CODE_GENERATION_RECORD_CODEC_ID,
        }
    }
}

impl ItemCodec<CodeGenerationRecordDto> for CodeGenerationRecordCodec {
    fn codec_id(&self) -> StableCodecId {
        self.codec_id
    }

    fn encode(&self, value: &CodeGenerationRecordDto) -> io::Result<Vec<u8>> {
        validate_code_generation_record(value)?;
        let mut encoder = Encoder::default();
        encoder.write_u32(u32::from(value.runtime_requirements));
        match &value.source {
            CodeGenerationSourceDto::Raw { source } => {
                encoder.write_u8(0);
                encoder.write_record_string(source)?;
            }
            CodeGenerationSourceDto::OriginalWithReplacements {
                prefix,
                original_source_len,
                original_name,
                replacements,
                suffix,
            } => {
                encoder.write_u8(1);
                encoder.write_record_string(prefix)?;
                encoder.write_u32(*original_source_len);
                encoder.write_record_string(original_name)?;
                encoder.write_count(replacements.len())?;
                for replacement in replacements {
                    encoder.write_u32(replacement.start);
                    encoder.write_u32(replacement.end);
                    encoder.write_record_string(&replacement.content)?;
                    encoder.write_optional_record_string(replacement.name.as_deref())?;
                    encoder.write_u8(match replacement.enforce {
                        ReplacementEnforceDto::Pre => 0,
                        ReplacementEnforceDto::Normal => 1,
                        ReplacementEnforceDto::Post => 2,
                    });
                }
                encoder.write_record_string(suffix)?;
            }
        }
        Ok(encoder.finish())
    }

    fn decode(&self, bytes: &[u8]) -> Option<CodeGenerationRecordDto> {
        let mut decoder = Decoder::new(bytes);
        let runtime_requirements = u16::try_from(decoder.read_u32()?).ok()?;
        let source = match decoder.read_u8()? {
            0 => CodeGenerationSourceDto::Raw {
                source: decoder.read_record_string()?,
            },
            1 => {
                let prefix = decoder.read_record_string()?;
                let original_source_len = decoder.read_u32()?;
                let original_name = decoder.read_record_string()?;
                let replacement_count = decoder.read_count()?;
                let mut replacements = Vec::with_capacity(replacement_count);
                for _ in 0..replacement_count {
                    replacements.push(CodeGenerationReplacementDto {
                        start: decoder.read_u32()?,
                        end: decoder.read_u32()?,
                        content: decoder.read_record_string()?,
                        name: decoder.read_optional_record_string()?,
                        enforce: match decoder.read_u8()? {
                            0 => ReplacementEnforceDto::Pre,
                            1 => ReplacementEnforceDto::Normal,
                            2 => ReplacementEnforceDto::Post,
                            _ => return None,
                        },
                    });
                }
                CodeGenerationSourceDto::OriginalWithReplacements {
                    prefix,
                    original_source_len,
                    original_name,
                    replacements,
                    suffix: decoder.read_record_string()?,
                }
            }
            _ => return None,
        };
        decoder.finish()?;
        let record = CodeGenerationRecordDto {
            source,
            runtime_requirements,
        };
        validate_code_generation_record(&record).ok()?;
        Some(record)
    }
}

fn validate_code_generation_record(record: &CodeGenerationRecordDto) -> io::Result<()> {
    if record.runtime_requirements & !RuntimeRequirements::valid_mask() != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Code Generation Record contains unknown Runtime Requirements",
        ));
    }
    let CodeGenerationSourceDto::OriginalWithReplacements {
        original_source_len,
        replacements,
        ..
    } = &record.source
    else {
        return Ok(());
    };
    for replacement in replacements {
        let start = usize::try_from(replacement.start).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Code Generation Record replacement start is invalid",
            )
        })?;
        let end = usize::try_from(replacement.end).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Code Generation Record replacement end is invalid",
            )
        })?;
        if start > end || end > usize::try_from(*original_source_len).unwrap_or(usize::MAX) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Code Generation Record contains an invalid replacement range",
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AssetRenderRecordCodec {
    codec_id: StableCodecId,
}

impl AssetRenderRecordCodec {
    pub(crate) const fn current() -> Self {
        Self {
            codec_id: ASSET_RENDER_RECORD_CODEC_ID,
        }
    }
}

impl ItemCodec<AssetRenderRecordDto> for AssetRenderRecordCodec {
    fn codec_id(&self) -> StableCodecId {
        self.codec_id
    }

    fn encode(&self, value: &AssetRenderRecordDto) -> io::Result<Vec<u8>> {
        let mut encoder = Encoder::default();
        encoder.write_record_string(&value.source)?;
        encoder.write_optional_record_string(value.source_map.as_deref())?;
        Ok(encoder.finish())
    }

    fn decode(&self, bytes: &[u8]) -> Option<AssetRenderRecordDto> {
        let mut decoder = Decoder::new(bytes);
        let record = AssetRenderRecordDto {
            source: decoder.read_record_string()?,
            source_map: decoder.read_optional_record_string()?,
        };
        decoder.finish()?;
        Some(record)
    }
}

fn encode_module_build_record(record: &ModuleBuildRecordDto) -> io::Result<Vec<u8>> {
    validate_module_build_record(record)?;

    let mut encoder = Encoder::default();
    encoder.write_string(&record.source)?;
    encoder.write_optional_bytes(record.binary_source.as_deref())?;
    encoder.write_u64(record.source_hash);
    encode_parsed_module(&mut encoder, &record.parsed)?;
    encode_snapshot(&mut encoder, &record.snapshot)?;
    encoder.write_u8(match record.parsed.build_side_effect_free {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    });
    Ok(encoder.finish())
}

fn decode_module_build_record(bytes: &[u8]) -> Option<ModuleBuildRecordDto> {
    let mut decoder = Decoder::new(bytes);
    let source = decoder.read_string()?;
    let binary_source = decoder.read_optional_bytes()?;
    let source_hash = decoder.read_u64()?;
    let mut parsed = decode_parsed_module(&mut decoder)?;
    let snapshot = decode_snapshot(&mut decoder)?;
    parsed.build_side_effect_free = match decoder.read_u8()? {
        0 => None,
        1 => Some(false),
        2 => Some(true),
        _ => return None,
    };
    decoder.finish()?;
    let record = ModuleBuildRecordDto {
        parsed,
        source,
        binary_source,
        source_hash,
        snapshot,
    };
    validate_module_build_record(&record).ok()?;
    Some(record)
}

fn encode_parsed_module(encoder: &mut Encoder, parsed: &ParsedModuleDto) -> io::Result<()> {
    encode_dependencies(encoder, &parsed.dependencies)?;
    encoder.write_count(parsed.blocks.len())?;
    for block in &parsed.blocks {
        encode_dependencies(encoder, &block.dependencies)?;
    }
    encode_dependencies(encoder, &parsed.presentational_dependencies)?;
    match &parsed.data {
        ParsedModuleDataDto::JavaScript => encoder.write_u8(0),
        ParsedModuleDataDto::Json(value) => {
            encoder.write_u8(1);
            encoder.write_record_string(value)?;
        }
        ParsedModuleDataDto::Asset { module_type } => {
            encoder.write_u8(2);
            encoder.write_u8(match module_type {
                ModuleTypeDto::JavaScriptAuto => 0,
                ModuleTypeDto::Json => 1,
                ModuleTypeDto::Asset => 2,
                ModuleTypeDto::AssetResource => 3,
                ModuleTypeDto::AssetInline => 4,
                ModuleTypeDto::AssetSource => 5,
            });
        }
    }
    Ok(())
}

fn decode_parsed_module(decoder: &mut Decoder<'_>) -> Option<ParsedModuleDto> {
    let dependencies = decode_dependencies(decoder)?;
    let block_count = decoder.read_count()?;
    let mut blocks = Vec::with_capacity(block_count);
    for _ in 0..block_count {
        blocks.push(AsyncDependenciesBlockDto {
            dependencies: decode_dependencies(decoder)?,
        });
    }
    let presentational_dependencies = decode_dependencies(decoder)?;
    let data = match decoder.read_u8()? {
        0 => ParsedModuleDataDto::JavaScript,
        1 => ParsedModuleDataDto::Json(decoder.read_record_string()?),
        2 => ParsedModuleDataDto::Asset {
            module_type: match decoder.read_u8()? {
                2 => ModuleTypeDto::Asset,
                3 => ModuleTypeDto::AssetResource,
                4 => ModuleTypeDto::AssetInline,
                5 => ModuleTypeDto::AssetSource,
                _ => return None,
            },
        },
        _ => return None,
    };
    Some(ParsedModuleDto {
        dependencies,
        blocks,
        presentational_dependencies,
        data,
        build_side_effect_free: None,
    })
}

fn encode_dependencies(encoder: &mut Encoder, dependencies: &[DependencyDto]) -> io::Result<()> {
    encoder.write_count(dependencies.len())?;
    for dependency in dependencies {
        encode_dependency(encoder, dependency)?;
    }
    Ok(())
}

fn decode_dependencies(decoder: &mut Decoder<'_>) -> Option<Vec<DependencyDto>> {
    let count = decoder.read_count()?;
    let mut dependencies = Vec::with_capacity(count);
    for _ in 0..count {
        dependencies.push(decode_dependency(decoder)?);
    }
    Some(dependencies)
}

fn encode_dependency(encoder: &mut Encoder, dependency: &DependencyDto) -> io::Result<()> {
    match dependency {
        DependencyDto::Entry { module } => {
            encoder.write_u8(0);
            encode_module_dependency(encoder, module)
        }
        DependencyDto::HarmonyImportSideEffect { module, import_var } => {
            encoder.write_u8(1);
            encode_module_dependency(encoder, module)?;
            encoder.write_optional_string(import_var.as_deref())
        }
        DependencyDto::HarmonyImportSpecifier {
            module,
            ids,
            name,
            usage_range,
            shorthand,
        } => {
            encoder.write_u8(2);
            encode_module_dependency(encoder, module)?;
            encoder.write_strings(ids)?;
            encoder.write_string(name)?;
            encode_source_range(encoder, *usage_range);
            encoder.write_bool(*shorthand);
            Ok(())
        }
        DependencyDto::HarmonyExportHeader {
            declaration_range,
            statement_range,
        } => {
            encoder.write_u8(3);
            encode_optional_source_range(encoder, *declaration_range);
            encode_source_range(encoder, *statement_range);
            Ok(())
        }
        DependencyDto::HarmonyExportSpecifier { id, name } => {
            encoder.write_u8(4);
            encoder.write_string(id)?;
            encoder.write_string(name)
        }
        DependencyDto::HarmonyExportExpression {
            range,
            statement_range,
            declaration_id,
        } => {
            encoder.write_u8(5);
            encode_source_range(encoder, *range);
            encode_source_range(encoder, *statement_range);
            encoder.write_optional_string(declaration_id.as_deref())
        }
        DependencyDto::HarmonyExportImportedSpecifier {
            module,
            ids,
            name,
            is_star,
        } => {
            encoder.write_u8(6);
            encode_module_dependency(encoder, module)?;
            encoder.write_strings(ids)?;
            encoder.write_optional_string(name.as_deref())?;
            encoder.write_bool(*is_star);
            Ok(())
        }
        DependencyDto::Null => {
            encoder.write_u8(7);
            Ok(())
        }
        DependencyDto::Const { expression, range } => {
            encoder.write_u8(8);
            encoder.write_string(expression)?;
            encode_source_range(encoder, *range);
            Ok(())
        }
        DependencyDto::Import { module } => {
            encoder.write_u8(9);
            encode_module_dependency(encoder, module)
        }
    }
}

fn decode_dependency(decoder: &mut Decoder<'_>) -> Option<DependencyDto> {
    Some(match decoder.read_u8()? {
        0 => DependencyDto::Entry {
            module: decode_module_dependency(decoder)?,
        },
        1 => DependencyDto::HarmonyImportSideEffect {
            module: decode_module_dependency(decoder)?,
            import_var: decoder.read_optional_string()?,
        },
        2 => DependencyDto::HarmonyImportSpecifier {
            module: decode_module_dependency(decoder)?,
            ids: decoder.read_strings()?,
            name: decoder.read_string()?,
            usage_range: decode_source_range(decoder)?,
            shorthand: decoder.read_bool()?,
        },
        3 => DependencyDto::HarmonyExportHeader {
            declaration_range: decode_optional_source_range(decoder)?,
            statement_range: decode_source_range(decoder)?,
        },
        4 => DependencyDto::HarmonyExportSpecifier {
            id: decoder.read_string()?,
            name: decoder.read_string()?,
        },
        5 => DependencyDto::HarmonyExportExpression {
            range: decode_source_range(decoder)?,
            statement_range: decode_source_range(decoder)?,
            declaration_id: decoder.read_optional_string()?,
        },
        6 => DependencyDto::HarmonyExportImportedSpecifier {
            module: decode_module_dependency(decoder)?,
            ids: decoder.read_strings()?,
            name: decoder.read_optional_string()?,
            is_star: decoder.read_bool()?,
        },
        7 => DependencyDto::Null,
        8 => DependencyDto::Const {
            expression: decoder.read_string()?,
            range: decode_source_range(decoder)?,
        },
        9 => DependencyDto::Import {
            module: decode_module_dependency(decoder)?,
        },
        _ => return None,
    })
}

fn encode_module_dependency(
    encoder: &mut Encoder,
    dependency: &ModuleDependencyDto,
) -> io::Result<()> {
    encoder.write_string(&dependency.request)?;
    encoder.write_string(&dependency.user_request)?;
    encoder.write_optional_u64(dependency.source_order);
    encode_optional_source_range(encoder, dependency.range);
    encoder.write_bool(dependency.weak);
    Ok(())
}

fn decode_module_dependency(decoder: &mut Decoder<'_>) -> Option<ModuleDependencyDto> {
    Some(ModuleDependencyDto {
        request: decoder.read_string()?,
        user_request: decoder.read_string()?,
        source_order: decoder.read_optional_u64()?,
        range: decode_optional_source_range(decoder)?,
        weak: decoder.read_bool()?,
    })
}

fn encode_source_range(encoder: &mut Encoder, range: SourceRangeDto) {
    encoder.write_u32(range.start);
    encoder.write_u32(range.end);
}

fn decode_source_range(decoder: &mut Decoder<'_>) -> Option<SourceRangeDto> {
    Some(SourceRangeDto {
        start: decoder.read_u32()?,
        end: decoder.read_u32()?,
    })
}

fn encode_optional_source_range(encoder: &mut Encoder, range: Option<SourceRangeDto>) {
    match range {
        Some(range) => {
            encoder.write_u8(1);
            encode_source_range(encoder, range);
        }
        None => encoder.write_u8(0),
    }
}

fn decode_optional_source_range(decoder: &mut Decoder<'_>) -> Option<Option<SourceRangeDto>> {
    match decoder.read_u8()? {
        0 => Some(None),
        1 => Some(Some(decode_source_range(decoder)?)),
        _ => None,
    }
}

fn parsed_module_ranges_are_valid(parsed: &ParsedModuleDto, source: &str) -> bool {
    parsed
        .dependencies
        .iter()
        .chain(
            parsed
                .blocks
                .iter()
                .flat_map(|block| block.dependencies.iter()),
        )
        .chain(parsed.presentational_dependencies.iter())
        .all(|dependency| dependency_ranges_are_valid(dependency, source))
}

fn dependency_ranges_are_valid(dependency: &DependencyDto, source: &str) -> bool {
    let module_range_is_valid = |module: &ModuleDependencyDto| {
        module.source_order.is_none_or(|source_order| {
            usize::try_from(source_order)
                .is_ok_and(|source_order| source_order <= MAX_COLLECTION_ENTRIES)
        }) && module
            .range
            .is_none_or(|range| source_range_is_valid(range, source))
    };
    match dependency {
        DependencyDto::Entry { module }
        | DependencyDto::HarmonyImportSideEffect { module, .. }
        | DependencyDto::HarmonyExportImportedSpecifier { module, .. } => {
            module_range_is_valid(module)
        }
        DependencyDto::Import { module } => module.range.is_some() && module_range_is_valid(module),
        DependencyDto::HarmonyImportSpecifier {
            module,
            usage_range,
            ..
        } => module_range_is_valid(module) && source_range_is_valid(*usage_range, source),
        DependencyDto::HarmonyExportHeader {
            declaration_range,
            statement_range,
        } => {
            declaration_range.is_none_or(|range| source_range_is_valid(range, source))
                && source_range_is_valid(*statement_range, source)
        }
        DependencyDto::HarmonyExportExpression {
            range,
            statement_range,
            ..
        } => {
            source_range_is_valid(*range, source) && source_range_is_valid(*statement_range, source)
        }
        DependencyDto::Const { range, .. } => source_range_is_valid(*range, source),
        DependencyDto::HarmonyExportSpecifier { .. } | DependencyDto::Null => true,
    }
}

fn source_range_is_valid(range: SourceRangeDto, source: &str) -> bool {
    let start = usize::try_from(range.start).ok();
    let end = usize::try_from(range.end).ok();
    match (start, end) {
        (Some(start), Some(end)) => {
            start <= end
                && end <= source.len()
                && source.is_char_boundary(start)
                && source.is_char_boundary(end)
        }
        _ => false,
    }
}

fn encode_resolve_record(record: &ResolveRecordDto) -> io::Result<Vec<u8>> {
    let mut encoder = Encoder::default();
    encoder.write_u8(match record.identity.module_type {
        ModuleTypeDto::JavaScriptAuto => 0,
        ModuleTypeDto::Json => 1,
        ModuleTypeDto::Asset => 2,
        ModuleTypeDto::AssetResource => 3,
        ModuleTypeDto::AssetInline => 4,
        ModuleTypeDto::AssetSource => 5,
    });
    encoder.write_path(&record.identity.resource)?;
    encoder.write_optional_string(record.identity.query.as_deref())?;
    encoder.write_optional_string(record.identity.fragment.as_deref())?;
    encoder.write_optional_string(record.identity.layer.as_deref())?;
    encoder.write_strings(&record.identity.loaders)?;
    encoder.write_path(&record.resource)?;
    encoder.write_paths(&record.file_dependencies)?;
    encoder.write_paths(&record.context_dependencies)?;
    encoder.write_paths(&record.missing_dependencies)?;
    encode_snapshot(&mut encoder, &record.snapshot)?;
    Ok(encoder.finish())
}

fn encode_snapshot(encoder: &mut Encoder, snapshot: &SnapshotDto) -> io::Result<()> {
    encoder.write_count(snapshot.entries.len())?;
    for entry in &snapshot.entries {
        match entry {
            SnapshotEntryDto::File {
                path,
                exists,
                modified,
                source_hash,
            } => {
                encoder.write_u8(0);
                encoder.write_path(path)?;
                encoder.write_bool(*exists);
                encoder.write_optional_timestamp(*modified)?;
                encoder.write_optional_u64(*source_hash);
            }
            SnapshotEntryDto::Context {
                path,
                exists,
                timestamp_hash,
                content_hash,
            } => {
                encoder.write_u8(1);
                encoder.write_path(path)?;
                encoder.write_bool(*exists);
                encoder.write_optional_u64(*timestamp_hash);
                encoder.write_optional_u64(*content_hash);
            }
            SnapshotEntryDto::MissingExistence { path } => {
                encoder.write_u8(2);
                encoder.write_path(path)?;
            }
            SnapshotEntryDto::ImmutablePath { path } => {
                encoder.write_u8(3);
                encoder.write_path(path)?;
            }
            SnapshotEntryDto::ManagedPath { path, root, state } => {
                encoder.write_u8(4);
                encoder.write_path(path)?;
                encoder.write_path(root)?;
                match state {
                    ManagedItemStateDto::NodeModules => encoder.write_u8(0),
                    ManagedItemStateDto::GroupingFolder => encoder.write_u8(1),
                    ManagedItemStateDto::Package { name, version } => {
                        encoder.write_u8(2);
                        encoder.write_string(name)?;
                        encoder.write_string(version)?;
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn encode_pack_file_guard(guard: &PackFileGuardDto) -> io::Result<Vec<u8>> {
    let mut encoder = Encoder::default();
    encoder.write_bytes(&guard.version)?;
    encode_snapshot(&mut encoder, &guard.build_dependencies)?;
    encode_snapshot(&mut encoder, &guard.resolve_build_dependencies)?;
    Ok(encoder.finish())
}

pub(super) fn decode_pack_file_guard(bytes: &[u8]) -> Option<PackFileGuardDto> {
    let mut decoder = Decoder::new(bytes);
    let guard = PackFileGuardDto {
        version: decoder.read_bytes()?,
        build_dependencies: decode_snapshot(&mut decoder)?,
        resolve_build_dependencies: decode_snapshot(&mut decoder)?,
    };
    decoder.finish()?;
    Some(guard)
}

fn decode_resolve_record(bytes: &[u8]) -> Option<ResolveRecordDto> {
    let mut decoder = Decoder::new(bytes);
    let module_type = match decoder.read_u8()? {
        0 => ModuleTypeDto::JavaScriptAuto,
        1 => ModuleTypeDto::Json,
        2 => ModuleTypeDto::Asset,
        3 => ModuleTypeDto::AssetResource,
        4 => ModuleTypeDto::AssetInline,
        5 => ModuleTypeDto::AssetSource,
        _ => return None,
    };
    let identity = ModuleIdentityDto {
        module_type,
        resource: decoder.read_path()?,
        query: decoder.read_optional_string()?,
        fragment: decoder.read_optional_string()?,
        layer: decoder.read_optional_string()?,
        loaders: decoder.read_strings()?,
    };
    let resource = decoder.read_path()?;
    let file_dependencies = decoder.read_paths()?;
    let context_dependencies = decoder.read_paths()?;
    let missing_dependencies = decoder.read_paths()?;
    let snapshot = decode_snapshot(&mut decoder)?;
    decoder.finish()?;
    Some(ResolveRecordDto {
        identity,
        resource,
        file_dependencies,
        context_dependencies,
        missing_dependencies,
        snapshot,
    })
}

fn decode_snapshot(decoder: &mut Decoder<'_>) -> Option<SnapshotDto> {
    let entry_count = decoder.read_count()?;
    let mut entries = Vec::with_capacity(entry_count);
    for _ in 0..entry_count {
        entries.push(match decoder.read_u8()? {
            0 => SnapshotEntryDto::File {
                path: decoder.read_path()?,
                exists: decoder.read_bool()?,
                modified: decoder.read_optional_timestamp()?,
                source_hash: decoder.read_optional_u64()?,
            },
            1 => SnapshotEntryDto::Context {
                path: decoder.read_path()?,
                exists: decoder.read_bool()?,
                timestamp_hash: decoder.read_optional_u64()?,
                content_hash: decoder.read_optional_u64()?,
            },
            2 => SnapshotEntryDto::MissingExistence {
                path: decoder.read_path()?,
            },
            3 => SnapshotEntryDto::ImmutablePath {
                path: decoder.read_path()?,
            },
            4 => {
                let path = decoder.read_path()?;
                let root = decoder.read_path()?;
                let state = match decoder.read_u8()? {
                    0 => ManagedItemStateDto::NodeModules,
                    1 => ManagedItemStateDto::GroupingFolder,
                    2 => ManagedItemStateDto::Package {
                        name: decoder.read_string()?,
                        version: decoder.read_string()?,
                    },
                    _ => return None,
                };
                SnapshotEntryDto::ManagedPath { path, root, state }
            }
            _ => return None,
        });
    }
    Some(SnapshotDto { entries })
}

#[derive(Debug, Default)]
struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn write_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn write_bool(&mut self, value: bool) {
        self.write_u8(u8::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn write_i64(&mut self, value: i64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn write_count(&mut self, value: usize) -> io::Result<()> {
        if value > MAX_COLLECTION_ENTRIES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "PackFile collection exceeds the configured bound",
            ));
        }
        let value = u32::try_from(value).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "PackFile collection is too large",
            )
        })?;
        self.write_u32(value);
        Ok(())
    }

    fn write_bytes(&mut self, value: &[u8]) -> io::Result<()> {
        self.write_bytes_with_limit(value, MAX_FIELD_BYTES)
    }

    fn write_bytes_with_limit(&mut self, value: &[u8], limit: usize) -> io::Result<()> {
        if value.len() > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "PackFile field exceeds the configured bound",
            ));
        }
        let length = u32::try_from(value.len()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "PackFile field is too large")
        })?;
        self.write_u32(length);
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    fn write_optional_bytes(&mut self, value: Option<&[u8]>) -> io::Result<()> {
        match value {
            Some(value) => {
                self.write_u8(1);
                self.write_bytes(value)
            }
            None => {
                self.write_u8(0);
                Ok(())
            }
        }
    }

    fn write_string(&mut self, value: &str) -> io::Result<()> {
        self.write_bytes(value.as_bytes())
    }

    fn write_record_string(&mut self, value: &str) -> io::Result<()> {
        self.write_bytes_with_limit(value.as_bytes(), MAX_RECORD_BYTES)
    }

    fn write_path(&mut self, value: &PathBytes) -> io::Result<()> {
        self.write_bytes(&value.0)
    }

    fn write_optional_string(&mut self, value: Option<&str>) -> io::Result<()> {
        match value {
            Some(value) => {
                self.write_u8(1);
                self.write_string(value)
            }
            None => {
                self.write_u8(0);
                Ok(())
            }
        }
    }

    fn write_optional_record_string(&mut self, value: Option<&str>) -> io::Result<()> {
        match value {
            Some(value) => {
                self.write_u8(1);
                self.write_record_string(value)
            }
            None => {
                self.write_u8(0);
                Ok(())
            }
        }
    }

    fn write_optional_u64(&mut self, value: Option<u64>) {
        match value {
            Some(value) => {
                self.write_u8(1);
                self.write_u64(value);
            }
            None => self.write_u8(0),
        }
    }

    fn write_optional_timestamp(&mut self, value: Option<TimestampDto>) -> io::Result<()> {
        match value {
            Some(value) if value.nanoseconds < 1_000_000_000 => {
                self.write_u8(1);
                self.write_i64(value.seconds);
                self.write_u32(value.nanoseconds);
                Ok(())
            }
            Some(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "PackFile timestamp nanoseconds are out of range",
            )),
            None => {
                self.write_u8(0);
                Ok(())
            }
        }
    }

    fn write_strings(&mut self, values: &[String]) -> io::Result<()> {
        self.write_count(values.len())?;
        for value in values {
            self.write_string(value)?;
        }
        Ok(())
    }

    fn write_paths(&mut self, values: &[PathBytes]) -> io::Result<()> {
        self.write_count(values.len())?;
        for value in values {
            self.write_path(value)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct Decoder<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Decoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn finish(self) -> Option<()> {
        (self.position == self.bytes.len()).then_some(())
    }

    fn read_exact<const N: usize>(&mut self) -> Option<[u8; N]> {
        let end = self.position.checked_add(N)?;
        let bytes = self.bytes.get(self.position..end)?;
        self.position = end;
        bytes.try_into().ok()
    }

    fn read_u8(&mut self) -> Option<u8> {
        Some(self.read_exact::<1>()?[0])
    }

    fn read_bool(&mut self) -> Option<bool> {
        match self.read_u8()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    fn read_u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.read_exact()?))
    }

    fn read_u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.read_exact()?))
    }

    fn read_i64(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.read_exact()?))
    }

    fn read_count(&mut self) -> Option<usize> {
        let count = usize::try_from(self.read_u32()?).ok()?;
        (count <= MAX_COLLECTION_ENTRIES).then_some(count)
    }

    fn read_bytes(&mut self) -> Option<Vec<u8>> {
        self.read_bytes_with_limit(MAX_FIELD_BYTES)
    }

    fn read_bytes_with_limit(&mut self, limit: usize) -> Option<Vec<u8>> {
        let length = usize::try_from(self.read_u32()?).ok()?;
        if length > limit {
            return None;
        }
        let end = self.position.checked_add(length)?;
        let bytes = self.bytes.get(self.position..end)?.to_vec();
        self.position = end;
        Some(bytes)
    }

    fn read_optional_bytes(&mut self) -> Option<Option<Vec<u8>>> {
        match self.read_u8()? {
            0 => Some(None),
            1 => Some(Some(self.read_bytes()?)),
            _ => None,
        }
    }

    fn read_string(&mut self) -> Option<String> {
        String::from_utf8(self.read_bytes()?).ok()
    }

    fn read_record_string(&mut self) -> Option<String> {
        String::from_utf8(self.read_bytes_with_limit(MAX_RECORD_BYTES)?).ok()
    }

    fn read_path(&mut self) -> Option<PathBytes> {
        Some(PathBytes(self.read_bytes()?))
    }

    fn read_optional_string(&mut self) -> Option<Option<String>> {
        match self.read_u8()? {
            0 => Some(None),
            1 => Some(Some(self.read_string()?)),
            _ => None,
        }
    }

    fn read_optional_record_string(&mut self) -> Option<Option<String>> {
        match self.read_u8()? {
            0 => Some(None),
            1 => Some(Some(self.read_record_string()?)),
            _ => None,
        }
    }

    fn read_optional_u64(&mut self) -> Option<Option<u64>> {
        match self.read_u8()? {
            0 => Some(None),
            1 => Some(Some(self.read_u64()?)),
            _ => None,
        }
    }

    fn read_optional_timestamp(&mut self) -> Option<Option<TimestampDto>> {
        match self.read_u8()? {
            0 => Some(None),
            1 => {
                let seconds = self.read_i64()?;
                let nanoseconds = self.read_u32()?;
                (nanoseconds < 1_000_000_000).then_some(Some(TimestampDto {
                    seconds,
                    nanoseconds,
                }))
            }
            _ => None,
        }
    }

    fn read_strings(&mut self) -> Option<Vec<String>> {
        let count = self.read_count()?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(self.read_string()?);
        }
        Some(values)
    }

    fn read_paths(&mut self) -> Option<Vec<PathBytes>> {
        let count = self.read_count()?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(self.read_path()?);
        }
        Some(values)
    }
}
