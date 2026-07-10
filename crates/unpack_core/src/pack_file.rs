#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use crate::cache_hash::stable_hash;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn resolve_records_round_trip_through_the_registered_private_codec()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let address = PackFileAddress::new("unpack/resolve", b"issuer:/project/src|request:./dep");
        let etag = PackFileETag::new(b"resolve-inputs-v1");
        let record = resolve_record("dep.js");
        let registry = CodecRegistry::new().with_resolve_record(ResolveRecordCodec::current());

        PackFile::publish_resolve_records(
            temp.path(),
            &registry,
            [(address.clone(), Some(etag.clone()), record.clone())],
        )?;

        let index_bytes = fs::read(temp.path().join(INDEX_FILE))?;
        assert!(index_bytes.starts_with(INDEX_MAGIC));
        assert!(!contains_bytes(&index_bytes, b"webpack"));
        assert!(!contains_bytes(&index_bytes, b"schema_version"));
        assert_eq!(RESOLVE_RECORD_TYPE_ID.as_bytes(), b"unpack.resolve.1");

        let mut pack_file = PackFile::open(temp.path(), registry);
        assert_eq!(pack_file.entry_count(), 1);
        assert_eq!(
            pack_file
                .get_resolve_record(&address, Some(&etag))
                .as_deref(),
            Some(&record)
        );

        Ok(())
    }

    #[test]
    fn opening_reads_only_the_index_and_get_decodes_only_requested_content()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let first = PackFileAddress::new("unpack/resolve", b"first");
        let second = PackFileAddress::new("unpack/resolve", b"second");
        let etag = PackFileETag::new(b"current");
        let registry = CodecRegistry::new().with_resolve_record(ResolveRecordCodec::current());
        PackFile::publish_resolve_records(
            temp.path(),
            &registry,
            [
                (
                    first.clone(),
                    Some(etag.clone()),
                    resolve_record("first.js"),
                ),
                (second, Some(etag.clone()), resolve_record("second.js")),
            ],
        )?;

        let mut pack_file = PackFile::open(temp.path(), registry);
        assert_eq!(pack_file.entry_count(), 2);
        assert_eq!(
            pack_file.read_stats(),
            PackFileReadStats {
                index_reads: 1,
                content_reads: 0,
                content_bytes_read: 0,
                decoded_records: 0,
            }
        );

        assert!(
            pack_file
                .get_resolve_record(&first, Some(&PackFileETag::new(b"stale")))
                .is_none()
        );
        assert_eq!(pack_file.read_stats().content_reads, 0);

        assert_eq!(
            pack_file.get_resolve_record(&first, Some(&etag)).as_deref(),
            Some(&resolve_record("first.js"))
        );
        let stats = pack_file.read_stats();
        assert_eq!(stats.index_reads, 1);
        assert_eq!(stats.content_reads, 1);
        assert!(stats.content_bytes_read > 0);
        assert_eq!(stats.decoded_records, 1);

        Ok(())
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct DummyItem(Vec<u8>);

    impl PackFileItem for DummyItem {
        const TYPE_ID: StableTypeId = StableTypeId::new(*b"unpack.dummy.001");
    }

    #[derive(Debug)]
    struct DummyCodec;

    impl ItemCodec<DummyItem> for DummyCodec {
        fn codec_id(&self) -> StableCodecId {
            StableCodecId::new(*b"unpack.dummy.c01")
        }

        fn encode(&self, value: &DummyItem) -> io::Result<Vec<u8>> {
            Ok(value.0.clone())
        }

        fn decode(&self, bytes: &[u8]) -> Option<DummyItem> {
            Some(DummyItem(bytes.to_vec()))
        }
    }

    #[test]
    fn another_item_family_uses_the_same_index_by_registering_only_a_codec()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let address = PackFileAddress::new("unpack/dummy", b"example");
        let registry = CodecRegistry::new().with_codec::<DummyItem, _>(DummyCodec);
        PackFile::publish_items(
            temp.path(),
            &registry,
            [(address.clone(), None, DummyItem(b"payload".to_vec()))],
        )?;

        let index = decode_index(&fs::read(temp.path().join(INDEX_FILE))?)
            .expect("decode generic PackFile index");
        assert_eq!(index.entries[&address].type_id, DummyItem::TYPE_ID);
        assert_eq!(index.entries[&address].codec_id, DummyCodec.codec_id());

        let mut pack_file = PackFile::open(temp.path(), registry);
        assert_eq!(
            pack_file.get::<DummyItem>(&address, None).as_deref(),
            Some(&DummyItem(b"payload".to_vec()))
        );
        Ok(())
    }

    #[test]
    fn module_build_records_round_trip_every_parsed_module_variant()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let address = PackFileAddress::new("unpack/module-build", b"module-identity");
        let record = module_build_record();
        let registry =
            CodecRegistry::new().with_module_build_record(ModuleBuildRecordCodec::current());

        PackFile::publish_module_build_records(
            temp.path(),
            &registry,
            [(address.clone(), None, record.clone())],
        )?;

        assert_eq!(MODULE_BUILD_RECORD_TYPE_ID.as_bytes(), b"unpack.moduleb.1");
        let mut pack_file = PackFile::open(temp.path(), registry);
        assert_eq!(
            pack_file.get_module_build_record(&address, None).as_deref(),
            Some(&record)
        );
        Ok(())
    }

    #[test]
    fn asset_render_records_round_trip_through_the_registered_private_codec()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let address = PackFileAddress::new("unpack/asset-render", b"initial:main");
        let etag = PackFileETag::new(b"exact-render-hash");
        let record = AssetRenderRecordDto {
            source: "console.log('cached render');\n".to_string(),
            source_map: Some(
                r#"{"version":3,"sources":["src/index.js"],"names":[],"mappings":"AAAA","sourcesContent":["console.log('cached render');\\n"]}"#
                    .to_string(),
            ),
        };
        let registry =
            CodecRegistry::new().with_asset_render_record(AssetRenderRecordCodec::current());

        PackFile::publish_items(
            temp.path(),
            &registry,
            [(address.clone(), Some(etag.clone()), record.clone())],
        )?;

        assert_eq!(ASSET_RENDER_RECORD_TYPE_ID.as_bytes(), b"unpack.asset-r.1");
        let mut pack_file = PackFile::open(temp.path(), registry);
        assert_eq!(
            pack_file
                .get::<AssetRenderRecordDto>(&address, Some(&etag))
                .as_deref(),
            Some(&record)
        );
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
    fn heterogeneous_batch_publishes_one_guarded_revision_and_honors_its_explicit_base()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let resolve_address = PackFileAddress::new("unpack/resolve", b"request");
        let module_address = PackFileAddress::new("unpack/module-build", b"identity");
        let registry = CodecRegistry::new()
            .with_resolve_record(ResolveRecordCodec::current())
            .with_module_build_record(ModuleBuildRecordCodec::current());
        let guard_v1 = PackFileGuardDto {
            version: b"v1".to_vec(),
            build_dependencies: resolve_record("build-dependency.js").snapshot,
            resolve_build_dependencies: resolve_record("resolve-build-dependency.js").snapshot,
        };
        let mut initial = PackFileWriteBatch::new();
        initial.insert(
            &registry,
            resolve_address.clone(),
            None,
            resolve_record("resolved.js"),
        )?;
        initial.insert(
            &registry,
            module_address.clone(),
            None,
            module_build_record(),
        )?;
        PackFile::publish_batch(
            temp.path(),
            Some(guard_v1.clone()),
            PublicationBase::ReplaceAll,
            initial,
        )?;

        let first_index = decode_index(&fs::read(PackFile::index_path(temp.path()))?)
            .expect("decode first heterogeneous index");
        assert_eq!(first_index.revision, 1);
        assert_eq!(first_index.guard.as_ref(), Some(&guard_v1));
        assert_eq!(first_index.entries.len(), 2);
        assert_eq!(
            first_index
                .entries
                .values()
                .map(|entry| &entry.content.file)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            1
        );

        let guard_v2 = PackFileGuardDto {
            version: b"v2".to_vec(),
            ..guard_v1
        };
        let mut update = PackFileWriteBatch::new();
        update.insert(
            &registry,
            resolve_address.clone(),
            None,
            resolve_record("resolved-v2.js"),
        )?;
        PackFile::publish_batch(
            temp.path(),
            Some(guard_v2.clone()),
            PublicationBase::PreserveEntries {
                expected_revision: 1,
            },
            update,
        )?;

        let mut reopened = PackFile::open(temp.path(), registry.clone());
        assert_eq!(reopened.revision(), 2);
        assert_eq!(reopened.guard(), Some(&guard_v2));
        assert_eq!(
            reopened
                .get_resolve_record(&resolve_address, None)
                .as_deref(),
            Some(&resolve_record("resolved-v2.js"))
        );
        assert_eq!(
            reopened
                .get_module_build_record(&module_address, None)
                .as_deref(),
            Some(&module_build_record())
        );

        let committed_index = fs::read(PackFile::index_path(temp.path()))?;
        let mut stale = PackFileWriteBatch::new();
        stale.insert(
            &registry,
            resolve_address.clone(),
            None,
            resolve_record("must-not-publish.js"),
        )?;
        assert!(
            PackFile::publish_batch(
                temp.path(),
                Some(guard_v2.clone()),
                PublicationBase::PreserveEntries {
                    expected_revision: 1,
                },
                stale,
            )
            .is_err()
        );
        assert_eq!(
            fs::read(PackFile::index_path(temp.path()))?,
            committed_index
        );

        let mut cold = PackFileWriteBatch::new();
        cold.insert(
            &registry,
            module_address.clone(),
            None,
            module_build_record(),
        )?;
        PackFile::publish_batch(
            temp.path(),
            Some(guard_v2),
            PublicationBase::ReplaceAll,
            cold,
        )?;
        let mut replaced = PackFile::open(temp.path(), registry);
        assert_eq!(replaced.revision(), 3);
        assert!(
            replaced
                .get_resolve_record(&resolve_address, None)
                .is_none()
        );
        assert!(
            replaced
                .get_module_build_record(&module_address, None)
                .is_some()
        );
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
        Ok(())
    }

    #[test]
    fn invalid_index_framing_and_length_bounds_open_as_an_empty_packfile()
    -> Result<(), Box<dyn std::error::Error>> {
        let (temp, _, _, _) = published_record("truncated.js")?;
        let index_path = temp.path().join(INDEX_FILE);
        let index = fs::read(&index_path)?;
        fs::write(&index_path, &index[..INDEX_MAGIC.len() + 8])?;
        assert_eq!(
            PackFile::open(
                temp.path(),
                CodecRegistry::new().with_resolve_record(ResolveRecordCodec::current())
            )
            .entry_count(),
            0
        );

        let (temp, _, _, _) = published_record("oversized.js")?;
        let index_path = temp.path().join(INDEX_FILE);
        fs::OpenOptions::new()
            .write(true)
            .open(&index_path)?
            .set_len(u64::try_from(MAX_INDEX_BYTES + 1)?)?;
        assert_eq!(
            PackFile::open(
                temp.path(),
                CodecRegistry::new().with_resolve_record(ResolveRecordCodec::current())
            )
            .entry_count(),
            0
        );

        Ok(())
    }

    #[test]
    fn invalid_content_framing_bounds_and_checksums_are_safe_misses()
    -> Result<(), Box<dyn std::error::Error>> {
        let (temp, address, etag, registry) = published_record("checksum.js")?;
        let content_file_path = content_path(temp.path(), &address);
        let mut content = fs::read(&content_file_path)?;
        *content
            .last_mut()
            .expect("content frame should not be empty") ^= 0xff;
        fs::write(&content_file_path, content)?;
        assert!(
            PackFile::open(temp.path(), registry)
                .get_resolve_record(&address, Some(&etag))
                .is_none()
        );

        let (temp, address, etag, registry) = published_record("framing.js")?;
        let content_file_path = content_path(temp.path(), &address);
        fs::write(&content_file_path, CONTENT_MAGIC)?;
        assert!(
            PackFile::open(temp.path(), registry)
                .get_resolve_record(&address, Some(&etag))
                .is_none()
        );

        let (temp, address, etag, registry) = published_record("bounds.js")?;
        let content_file_path = content_path(temp.path(), &address);
        fs::OpenOptions::new()
            .write(true)
            .open(&content_file_path)?
            .set_len(u64::try_from(MAX_CONTENT_BYTES + 1)?)?;
        assert!(
            PackFile::open(temp.path(), registry)
                .get_resolve_record(&address, Some(&etag))
                .is_none()
        );

        Ok(())
    }

    #[test]
    fn unknown_types_and_incompatible_codecs_miss_before_reading_content()
    -> Result<(), Box<dyn std::error::Error>> {
        let (temp, address, etag, _) = published_record("unknown.js")?;
        let mut unknown = PackFile::open(temp.path(), CodecRegistry::new());
        assert!(unknown.get_resolve_record(&address, Some(&etag)).is_none());
        assert_eq!(unknown.read_stats().content_reads, 0);

        let incompatible_id = StableCodecId(*b"unpack.rslv.c999");
        let registry = CodecRegistry::new()
            .with_resolve_record(ResolveRecordCodec::with_codec_id(incompatible_id));
        let mut incompatible = PackFile::open(temp.path(), registry);
        assert!(
            incompatible
                .get_resolve_record(&address, Some(&etag))
                .is_none()
        );
        assert_eq!(incompatible.read_stats().content_reads, 0);

        Ok(())
    }

    #[test]
    fn resolve_codec_covers_every_snapshot_variant_and_rejects_invalid_timestamps()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let address = PackFileAddress::new("unpack/resolve", b"all-snapshot-variants");
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
        let registry = CodecRegistry::new().with_resolve_record(ResolveRecordCodec::current());
        PackFile::publish_resolve_records(
            temp.path(),
            &registry,
            [(address.clone(), None, record.clone())],
        )?;
        assert_eq!(
            PackFile::open(temp.path(), registry)
                .get_resolve_record(&address, None)
                .as_deref(),
            Some(&record)
        );

        let mut invalid = resolve_record("invalid-time.js");
        if let SnapshotEntryDto::File { modified, .. } = &mut invalid.snapshot.entries[0] {
            *modified = Some(TimestampDto {
                seconds: 0,
                nanoseconds: 1_000_000_000,
            });
        }
        let registry = CodecRegistry::new().with_resolve_record(ResolveRecordCodec::current());
        assert!(
            PackFile::publish_resolve_records(
                temp.path(),
                &registry,
                [(
                    PackFileAddress::new("unpack/resolve", b"invalid-time"),
                    None,
                    invalid
                )],
            )
            .is_err()
        );

        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn path_dto_preserves_non_utf8_linux_path_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let path = PathBytes(vec![b'/', b'p', b'k', b'g', 0xff]);
        assert_eq!(
            path.to_path_buf()
                .expect("Linux path bytes should be recoverable")
                .as_os_str()
                .as_bytes(),
            path.0
        );
    }

    #[test]
    fn incremental_publication_preserves_lazy_refs_in_grouped_content_packs()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let first = PackFileAddress::new("unpack/resolve", b"first");
        let second = PackFileAddress::new("unpack/resolve", b"second");
        let registry = CodecRegistry::new().with_resolve_record(ResolveRecordCodec::current());
        PackFile::publish_resolve_records(
            temp.path(),
            &registry,
            [
                (first.clone(), None, resolve_record("first-v1.js")),
                (second.clone(), None, resolve_record("second.js")),
            ],
        )?;

        let first_index = decode_index(&fs::read(temp.path().join(INDEX_FILE))?)
            .expect("decode first PackFile index");
        let content_files = first_index
            .entries
            .values()
            .map(|entry| entry.content.file.clone())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(content_files.len(), 1);
        let preserved_second_ref = first_index
            .entries
            .get(&second)
            .expect("second Resolve Record should be indexed")
            .content
            .clone();
        assert_ne!(
            first_index.entries[&first].content.offset,
            preserved_second_ref.offset
        );

        PackFile::publish_resolve_records(
            temp.path(),
            &registry,
            [(first.clone(), None, resolve_record("first-v2.js"))],
        )?;
        let second_index = decode_index(&fs::read(temp.path().join(INDEX_FILE))?)
            .expect("decode second PackFile index");
        assert_eq!(second_index.entries.len(), 2);
        assert_eq!(second_index.entries[&second].content, preserved_second_ref);
        assert_ne!(
            second_index.entries[&first].content.file,
            preserved_second_ref.file
        );

        let mut pack_file = PackFile::open(temp.path(), registry);
        assert_eq!(
            pack_file.get_resolve_record(&second, None).as_deref(),
            Some(&resolve_record("second.js"))
        );
        assert_eq!(
            pack_file.get_resolve_record(&first, None).as_deref(),
            Some(&resolve_record("first-v2.js"))
        );
        assert_eq!(
            pack_file.read_stats().content_bytes_read,
            usize::try_from(
                second_index.entries[&second].content.length
                    + second_index.entries[&first].content.length
            )?
        );

        Ok(())
    }

    #[test]
    fn publication_faults_preserve_the_last_committed_index_revision()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let address = PackFileAddress::new("unpack/resolve", b"stable");
        let registry = CodecRegistry::new().with_resolve_record(ResolveRecordCodec::current());
        let committed = resolve_record("committed.js");
        PackFile::publish_resolve_records(
            temp.path(),
            &registry,
            [(address.clone(), None, committed.clone())],
        )?;
        let committed_index = fs::read(temp.path().join(INDEX_FILE))?;

        for (fault, attempted_name) in [
            (PublishFault::AfterContentCommit, "after-content.js"),
            (PublishFault::BeforeIndexReplace, "before-index.js"),
        ] {
            assert!(
                PackFile::publish_resolve_records_with_fault(
                    temp.path(),
                    &registry,
                    [(address.clone(), None, resolve_record(attempted_name))],
                    fault,
                )
                .is_err()
            );
            assert_eq!(fs::read(temp.path().join(INDEX_FILE))?, committed_index);
            let mut reopened = PackFile::open(
                temp.path(),
                CodecRegistry::new().with_resolve_record(ResolveRecordCodec::current()),
            );
            assert_eq!(
                reopened.get_resolve_record(&address, None).as_deref(),
                Some(&committed)
            );
        }

        assert!(
            temp.path()
                .join(CONTENT_DIRECTORY)
                .join("pack-0000000000000002.bin")
                .exists()
        );
        Ok(())
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
            },
            source_hash: stable_hash(&source),
            source,
            snapshot: resolve_record("module.js").snapshot,
        }
    }

    fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    fn published_record(
        filename: &str,
    ) -> Result<
        (
            tempfile::TempDir,
            PackFileAddress,
            PackFileETag,
            CodecRegistry,
        ),
        Box<dyn std::error::Error>,
    > {
        let temp = tempdir()?;
        let address = PackFileAddress::new("unpack/resolve", filename.as_bytes());
        let etag = PackFileETag::new(b"current");
        let registry = CodecRegistry::new().with_resolve_record(ResolveRecordCodec::current());
        PackFile::publish_resolve_records(
            temp.path(),
            &registry,
            [(
                address.clone(),
                Some(etag.clone()),
                resolve_record(filename),
            )],
        )?;
        Ok((temp, address, etag, registry))
    }

    fn content_path(root: &Path, address: &PackFileAddress) -> PathBuf {
        root.join(
            decode_index(&fs::read(root.join(INDEX_FILE)).expect("read PackFile index"))
                .expect("decode PackFile index")
                .entries
                .get(address)
                .expect("PackFile address should exist")
                .content
                .file
                .clone(),
        )
    }
}
// Private PackFile storage primitives.
//
// This module is intentionally not selected by the production cache options yet. Its module
// boundary exists so the storage contract can be exercised before the backend cutover.

use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt, fs,
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    AsyncDependenciesBlock, ConstDependency, Dependency, EntryDependency,
    HarmonyExportExpressionDependency, HarmonyExportHeaderDependency,
    HarmonyExportImportedSpecifierDependency, HarmonyExportSpecifierDependency,
    HarmonyImportSideEffectDependency, HarmonyImportSpecifierDependency, ImportDependency,
    ModuleDependency, ModuleIdentity, ModuleType, NullDependency, SourceRange,
    build_cache::{ModuleBuildRecord, ResolveRecord},
    cache_hash::stable_hash,
    parser::ParsedModule,
    rendered_source::RenderedSource,
    snapshot::{PersistentManagedItemState, PersistentSnapshotEntry, Snapshot},
};

#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};

const INDEX_FILE: &str = "index.pack";
const CONTENT_DIRECTORY: &str = "content";
const INDEX_MAGIC: &[u8] = b"UNPACK-PACKFILE-INDEX\0";
const CONTENT_MAGIC: &[u8] = b"UNPACK-PACKFILE-CONTENT\0";
const MAX_INDEX_BYTES: usize = 16 * 1024 * 1024;
const MAX_CONTENT_BYTES: usize = 32 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;
const MAX_FIELD_BYTES: usize = 1024 * 1024;
const MAX_COLLECTION_ENTRIES: usize = 100_000;
const RESOLVE_RECORD_CODEC_ID: StableCodecId = StableCodecId::new(*b"unpack.rslv.c001");
const MODULE_BUILD_RECORD_CODEC_ID: StableCodecId = StableCodecId::new(*b"unpack.modb.c001");
const ASSET_RENDER_RECORD_CODEC_ID: StableCodecId = StableCodecId::new(*b"unpack.astr.c001");
pub(crate) const RESOLVE_RECORD_TYPE_ID: StableTypeId = StableTypeId::new(*b"unpack.resolve.1");
pub(crate) const MODULE_BUILD_RECORD_TYPE_ID: StableTypeId =
    StableTypeId::new(*b"unpack.moduleb.1");
pub(crate) const ASSET_RENDER_RECORD_TYPE_ID: StableTypeId =
    StableTypeId::new(*b"unpack.asset-r.1");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct StableTypeId([u8; 16]);

impl StableTypeId {
    pub(crate) const fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct StableCodecId([u8; 16]);

impl StableCodecId {
    pub(crate) const fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct PackFileAddress {
    namespace: Vec<u8>,
    identifier: Vec<u8>,
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
pub(crate) struct PackFileETag(Vec<u8>);

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
    pub(crate) fn to_path_buf(&self) -> Option<PathBuf> {
        Some(PathBuf::from(std::ffi::OsString::from_vec(self.0.clone())))
    }

    #[cfg(not(unix))]
    pub(crate) fn to_path_buf(&self) -> Option<PathBuf> {
        String::from_utf8(self.0.clone()).ok().map(PathBuf::from)
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
    pub(crate) source_hash: u64,
    pub(crate) snapshot: SnapshotDto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssetRenderRecordDto {
    pub(crate) source: String,
    pub(crate) source_map: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedModuleDto {
    pub(crate) dependencies: Vec<DependencyDto>,
    pub(crate) blocks: Vec<AsyncDependenciesBlockDto>,
    pub(crate) presentational_dependencies: Vec<DependencyDto>,
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

#[derive(Debug)]
struct PendingPackFileItem {
    etag: Option<PackFileETag>,
    type_id: StableTypeId,
    codec_id: StableCodecId,
    payload: Vec<u8>,
}

#[derive(Debug, Default)]
pub(crate) struct PackFileWriteBatch {
    items: BTreeMap<PackFileAddress, PendingPackFileItem>,
}

impl PackFileWriteBatch {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn insert<T: PackFileItem>(
        &mut self,
        registry: &CodecRegistry,
        address: PackFileAddress,
        etag: Option<PackFileETag>,
        value: T,
    ) -> io::Result<()> {
        let (codec_id, payload) = registry.encode(&value)?;
        self.items.insert(
            address,
            PendingPackFileItem {
                etag,
                type_id: T::TYPE_ID,
                codec_id,
                payload,
            },
        );
        Ok(())
    }
}

impl From<&ModuleIdentity> for ModuleIdentityDto {
    fn from(identity: &ModuleIdentity) -> Self {
        Self {
            module_type: match identity.module_type {
                ModuleType::JavaScriptAuto => ModuleTypeDto::JavaScriptAuto,
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
        let (parsed, source, source_hash) = record.cloned_parts();
        let source_hash = source_hash.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Module Build Record is missing its required source hash",
            )
        })?;
        let dto = Self {
            parsed: ParsedModuleDto::try_from(&parsed)?,
            source,
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
        validate_module_build_record(&record)?;
        Ok(ModuleBuildRecord::new(
            ParsedModule::try_from(&record.parsed)?,
            record.source,
            Snapshot::try_from(record.snapshot)?,
        ))
    }
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
    if record.source_hash != stable_hash(&record.source) {
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
    path.to_path_buf().ok_or_else(|| {
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
                .dependencies
                .iter()
                .map(dependency_to_dto)
                .collect::<io::Result<_>>()?,
            blocks: parsed
                .blocks
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
        })
    }
}

impl TryFrom<&ParsedModuleDto> for ParsedModule {
    type Error = io::Error;

    fn try_from(parsed: &ParsedModuleDto) -> io::Result<Self> {
        Ok(Self {
            dependencies: parsed
                .dependencies
                .iter()
                .map(dependency_from_dto)
                .collect::<io::Result<_>>()?,
            blocks: parsed
                .blocks
                .iter()
                .map(|block| {
                    Ok(AsyncDependenciesBlock::new(
                        block
                            .dependencies
                            .iter()
                            .map(dependency_from_dto)
                            .collect::<io::Result<_>>()?,
                    ))
                })
                .collect::<io::Result<_>>()?,
            presentational_dependencies: parsed
                .presentational_dependencies
                .iter()
                .map(dependency_from_dto)
                .collect::<io::Result<_>>()?,
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

fn dependency_from_dto(dependency: &DependencyDto) -> io::Result<Dependency> {
    Ok(match dependency {
        DependencyDto::Entry { module } => Dependency::Entry(EntryDependency {
            module: module_dependency_from_dto(module)?,
        }),
        DependencyDto::HarmonyImportSideEffect { module, import_var } => {
            Dependency::HarmonyImportSideEffect(HarmonyImportSideEffectDependency {
                module: module_dependency_from_dto(module)?,
                import_var: import_var.clone(),
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
            ids: ids.clone(),
            name: name.clone(),
            usage_range: (*usage_range).into(),
            shorthand: *shorthand,
        }),
        DependencyDto::HarmonyExportHeader {
            declaration_range,
            statement_range,
        } => Dependency::HarmonyExportHeader(HarmonyExportHeaderDependency {
            declaration_range: declaration_range.map(Into::into),
            statement_range: (*statement_range).into(),
        }),
        DependencyDto::HarmonyExportSpecifier { id, name } => {
            Dependency::HarmonyExportSpecifier(HarmonyExportSpecifierDependency {
                id: id.clone(),
                name: name.clone(),
            })
        }
        DependencyDto::HarmonyExportExpression {
            range,
            statement_range,
            declaration_id,
        } => Dependency::HarmonyExportExpression(HarmonyExportExpressionDependency {
            range: (*range).into(),
            statement_range: (*statement_range).into(),
            declaration_id: declaration_id.clone(),
        }),
        DependencyDto::HarmonyExportImportedSpecifier {
            module,
            ids,
            name,
            is_star,
        } => Dependency::HarmonyExportImportedSpecifier(HarmonyExportImportedSpecifierDependency {
            module: module_dependency_from_dto(module)?,
            ids: ids.clone(),
            name: name.clone(),
            is_star: *is_star,
        }),
        DependencyDto::Null => Dependency::Null(NullDependency),
        DependencyDto::Const { expression, range } => Dependency::Const(ConstDependency {
            expression: expression.clone(),
            range: (*range).into(),
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

fn module_dependency_from_dto(dependency: &ModuleDependencyDto) -> io::Result<ModuleDependency> {
    Ok(ModuleDependency {
        request: dependency.request.clone(),
        user_request: dependency.user_request.clone(),
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

pub(crate) trait PackFileItem: Clone + Send + Sync + 'static {
    const TYPE_ID: StableTypeId;
}

impl PackFileItem for ResolveRecordDto {
    const TYPE_ID: StableTypeId = RESOLVE_RECORD_TYPE_ID;
}

impl PackFileItem for ModuleBuildRecordDto {
    const TYPE_ID: StableTypeId = MODULE_BUILD_RECORD_TYPE_ID;
}

impl PackFileItem for AssetRenderRecordDto {
    const TYPE_ID: StableTypeId = ASSET_RENDER_RECORD_TYPE_ID;
}

pub(crate) trait ItemCodec<T: PackFileItem>: fmt::Debug + Send + Sync + 'static {
    fn codec_id(&self) -> StableCodecId;
    fn encode(&self, value: &T) -> io::Result<Vec<u8>>;
    fn decode(&self, bytes: &[u8]) -> Option<T>;
}

trait ErasedItemCodec: fmt::Debug + Send + Sync {
    fn codec_id(&self) -> StableCodecId;
    fn encode(&self, value: &dyn Any) -> io::Result<Vec<u8>>;
    fn decode(&self, bytes: &[u8]) -> Option<Box<dyn Any + Send + Sync>>;
}

struct CodecAdapter<T, C> {
    codec: C,
    marker: std::marker::PhantomData<fn() -> T>,
}

impl<T, C: fmt::Debug> fmt::Debug for CodecAdapter<T, C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodecAdapter")
            .field("codec", &self.codec)
            .finish_non_exhaustive()
    }
}

impl<T, C> ErasedItemCodec for CodecAdapter<T, C>
where
    T: PackFileItem,
    C: ItemCodec<T>,
{
    fn codec_id(&self) -> StableCodecId {
        self.codec.codec_id()
    }

    fn encode(&self, value: &dyn Any) -> io::Result<Vec<u8>> {
        let value = value.downcast_ref::<T>().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "PackFile item type mismatch")
        })?;
        self.codec.encode(value)
    }

    fn decode(&self, bytes: &[u8]) -> Option<Box<dyn Any + Send + Sync>> {
        self.codec
            .decode(bytes)
            .map(|value| Box::new(value) as Box<dyn Any + Send + Sync>)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodecRegistry {
    codecs: HashMap<StableTypeId, Arc<dyn ErasedItemCodec>>,
}

impl CodecRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_resolve_record(mut self, codec: ResolveRecordCodec) -> Self {
        self.register::<ResolveRecordDto, _>(codec);
        self
    }

    pub(crate) fn with_module_build_record(mut self, codec: ModuleBuildRecordCodec) -> Self {
        self.register::<ModuleBuildRecordDto, _>(codec);
        self
    }

    pub(crate) fn with_asset_render_record(mut self, codec: AssetRenderRecordCodec) -> Self {
        self.register::<AssetRenderRecordDto, _>(codec);
        self
    }

    #[allow(dead_code)]
    pub(crate) fn with_codec<T, C>(mut self, codec: C) -> Self
    where
        T: PackFileItem,
        C: ItemCodec<T>,
    {
        self.register::<T, C>(codec);
        self
    }

    fn register<T, C>(&mut self, codec: C)
    where
        T: PackFileItem,
        C: ItemCodec<T>,
    {
        self.codecs.insert(
            T::TYPE_ID,
            Arc::new(CodecAdapter::<T, C> {
                codec,
                marker: std::marker::PhantomData,
            }),
        );
    }

    fn encode<T: PackFileItem>(&self, value: &T) -> io::Result<(StableCodecId, Vec<u8>)> {
        let codec = self.codecs.get(&T::TYPE_ID).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "PackFile codec is not registered",
            )
        })?;
        let payload = codec.encode(value)?;
        if payload.len() > MAX_RECORD_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "PackFile record exceeds the configured bound",
            ));
        }
        Ok((codec.codec_id(), payload))
    }

    fn decode<T: PackFileItem>(
        &self,
        type_id: StableTypeId,
        codec_id: StableCodecId,
        bytes: &[u8],
    ) -> Option<T> {
        if type_id != T::TYPE_ID {
            return None;
        }
        let codec = self.codecs.get(&type_id)?;
        if codec.codec_id() != codec_id {
            return None;
        }
        Some(*codec.decode(bytes)?.downcast::<T>().ok()?)
    }
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

    #[cfg(test)]
    const fn with_codec_id(codec_id: StableCodecId) -> Self {
        Self { codec_id }
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
    encoder.write_u64(record.source_hash);
    encode_parsed_module(&mut encoder, &record.parsed)?;
    encode_snapshot(&mut encoder, &record.snapshot)?;
    Ok(encoder.finish())
}

fn decode_module_build_record(bytes: &[u8]) -> Option<ModuleBuildRecordDto> {
    let mut decoder = Decoder::new(bytes);
    let source = decoder.read_string()?;
    let source_hash = decoder.read_u64()?;
    let parsed = decode_parsed_module(&mut decoder)?;
    let snapshot = decode_snapshot(&mut decoder)?;
    decoder.finish()?;
    let record = ModuleBuildRecordDto {
        parsed,
        source,
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
    encode_dependencies(encoder, &parsed.presentational_dependencies)
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
    Some(ParsedModuleDto {
        dependencies,
        blocks,
        presentational_dependencies: decode_dependencies(decoder)?,
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

fn decode_resolve_record(bytes: &[u8]) -> Option<ResolveRecordDto> {
    let mut decoder = Decoder::new(bytes);
    let module_type = match decoder.read_u8()? {
        0 => ModuleTypeDto::JavaScriptAuto,
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

    fn write_raw(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
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

#[derive(Debug, Clone, Default)]
struct PackFileIndex {
    revision: u64,
    guard: Option<PackFileGuardDto>,
    entries: BTreeMap<PackFileAddress, PackFileIndexEntry>,
}

#[derive(Debug, Clone)]
struct PackFileIndexEntry {
    etag: Option<PackFileETag>,
    type_id: StableTypeId,
    codec_id: StableCodecId,
    content: ContentReference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ContentReference {
    file: PathBuf,
    offset: u64,
    length: u64,
    checksum: u64,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PackFileReadStats {
    index_reads: usize,
    content_reads: usize,
    content_bytes_read: usize,
    decoded_records: usize,
}

#[derive(Debug)]
pub(crate) struct PackFile {
    root: PathBuf,
    registry: CodecRegistry,
    index: PackFileIndex,
    #[cfg(test)]
    reads: PackFileReadStats,
}

impl PackFile {
    pub(crate) fn index_path(root: impl AsRef<Path>) -> PathBuf {
        root.as_ref().join(INDEX_FILE)
    }

    pub(crate) fn open(root: impl AsRef<Path>, registry: CodecRegistry) -> Self {
        let root = root.as_ref().to_path_buf();
        let index_path = Self::index_path(&root);
        let index = read_bounded(&index_path, MAX_INDEX_BYTES)
            .and_then(|bytes| decode_index(&bytes))
            .unwrap_or_default();
        Self {
            root,
            registry,
            index,
            #[cfg(test)]
            reads: PackFileReadStats {
                index_reads: usize::from(index_path.exists()),
                ..PackFileReadStats::default()
            },
        }
    }

    #[cfg(test)]
    fn entry_count(&self) -> usize {
        self.index.entries.len()
    }

    pub(crate) fn revision(&self) -> u64 {
        self.index.revision
    }

    pub(crate) fn guard(&self) -> Option<&PackFileGuardDto> {
        self.index.guard.as_ref()
    }

    pub(crate) fn get<T: PackFileItem>(
        &mut self,
        address: &PackFileAddress,
        etag: Option<&PackFileETag>,
    ) -> Option<Arc<T>> {
        let entry = self.index.entries.get(address)?.clone();
        if entry.etag.as_ref() != etag || entry.type_id != T::TYPE_ID {
            return None;
        }
        let codec = self.registry.codecs.get(&entry.type_id)?;
        if codec.codec_id() != entry.codec_id {
            return None;
        }

        let path = self.root.join(&entry.content.file);
        #[cfg(test)]
        {
            self.reads.content_reads += 1;
        }
        let frame = read_content_reference(&path, &entry.content)?;
        #[cfg(test)]
        {
            self.reads.content_bytes_read += frame.len();
        }
        if checksum(&frame) != entry.content.checksum {
            return None;
        }
        let (type_id, codec_id, payload) = decode_content(&frame)?;
        if type_id != entry.type_id || codec_id != entry.codec_id {
            return None;
        }
        let value = self.registry.decode::<T>(type_id, codec_id, payload)?;
        #[cfg(test)]
        {
            self.reads.decoded_records += 1;
        }
        Some(Arc::new(value))
    }

    pub(crate) fn get_resolve_record(
        &mut self,
        address: &PackFileAddress,
        etag: Option<&PackFileETag>,
    ) -> Option<Arc<ResolveRecordDto>> {
        self.get(address, etag)
    }

    pub(crate) fn get_module_build_record(
        &mut self,
        address: &PackFileAddress,
        etag: Option<&PackFileETag>,
    ) -> Option<Arc<ModuleBuildRecordDto>> {
        self.get(address, etag)
    }

    #[cfg(test)]
    fn publish_items<T, I>(
        root: impl AsRef<Path>,
        registry: &CodecRegistry,
        items: I,
    ) -> io::Result<()>
    where
        T: PackFileItem,
        I: IntoIterator<Item = (PackFileAddress, Option<PackFileETag>, T)>,
    {
        publish_items(root.as_ref(), registry, items, PublishFault::None)
    }

    pub(crate) fn publish_batch(
        root: impl AsRef<Path>,
        guard: Option<PackFileGuardDto>,
        base: PublicationBase,
        batch: PackFileWriteBatch,
    ) -> io::Result<()> {
        publish_batch(root.as_ref(), guard, base, batch, PublishFault::None)
    }

    #[cfg(test)]
    fn publish_resolve_records<I>(
        root: impl AsRef<Path>,
        registry: &CodecRegistry,
        records: I,
    ) -> io::Result<()>
    where
        I: IntoIterator<Item = (PackFileAddress, Option<PackFileETag>, ResolveRecordDto)>,
    {
        Self::publish_items(root, registry, records)
    }

    #[cfg(test)]
    fn publish_module_build_records<I>(
        root: impl AsRef<Path>,
        registry: &CodecRegistry,
        records: I,
    ) -> io::Result<()>
    where
        I: IntoIterator<Item = (PackFileAddress, Option<PackFileETag>, ModuleBuildRecordDto)>,
    {
        Self::publish_items(root, registry, records)
    }

    #[cfg(test)]
    fn read_stats(&self) -> PackFileReadStats {
        self.reads
    }

    #[cfg(test)]
    fn publish_resolve_records_with_fault<I>(
        root: impl AsRef<Path>,
        registry: &CodecRegistry,
        records: I,
        fault: PublishFault,
    ) -> io::Result<()>
    where
        I: IntoIterator<Item = (PackFileAddress, Option<PackFileETag>, ResolveRecordDto)>,
    {
        publish_items(root.as_ref(), registry, records, fault)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublishFault {
    None,
    AfterContentCommit,
    BeforeIndexReplace,
}

#[cfg(test)]
fn publish_items<T, I>(
    root: &Path,
    registry: &CodecRegistry,
    items: I,
    fault: PublishFault,
) -> io::Result<()>
where
    T: PackFileItem,
    I: IntoIterator<Item = (PackFileAddress, Option<PackFileETag>, T)>,
{
    let current = read_bounded(&root.join(INDEX_FILE), MAX_INDEX_BYTES)
        .and_then(|bytes| decode_index(&bytes))
        .unwrap_or_default();
    let mut batch = PackFileWriteBatch::new();
    for (address, etag, value) in items {
        batch.insert(registry, address, etag, value)?;
    }
    let expected_revision = current.revision;
    publish_batch(
        root,
        current.guard,
        PublicationBase::PreserveEntries { expected_revision },
        batch,
        fault,
    )
}

fn publish_batch(
    root: &Path,
    guard: Option<PackFileGuardDto>,
    base: PublicationBase,
    batch: PackFileWriteBatch,
    fault: PublishFault,
) -> io::Result<()> {
    let current = read_bounded(&root.join(INDEX_FILE), MAX_INDEX_BYTES)
        .and_then(|bytes| decode_index(&bytes))
        .unwrap_or_default();
    let mut entries = match base {
        PublicationBase::PreserveEntries { expected_revision }
            if current.revision == expected_revision =>
        {
            current.entries
        }
        PublicationBase::PreserveEntries { .. } => {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "PackFile publication base revision changed",
            ));
        }
        PublicationBase::ReplaceAll => BTreeMap::new(),
    };
    let revision = current
        .revision
        .checked_add(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "PackFile revision overflow"))?;

    fs::create_dir_all(root)?;
    let content_directory = root.join(CONTENT_DIRECTORY);
    fs::create_dir_all(&content_directory)?;
    let filename = format!("pack-{revision:016x}.bin");
    let relative_path = PathBuf::from(CONTENT_DIRECTORY).join(&filename);
    let mut content_pack = Vec::new();
    for (address, item) in batch.items {
        let frame = encode_content(item.type_id, item.codec_id, &item.payload)?;
        let offset = u64::try_from(content_pack.len()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "content pack offset is too large",
            )
        })?;
        let length = u64::try_from(frame.len()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "content frame is too large")
        })?;
        let frame_checksum = checksum(&frame);
        content_pack.extend_from_slice(&frame);
        if content_pack.len() > MAX_CONTENT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "content pack exceeds the configured bound",
            ));
        }
        entries.insert(
            address,
            PackFileIndexEntry {
                etag: item.etag,
                type_id: item.type_id,
                codec_id: item.codec_id,
                content: ContentReference {
                    file: relative_path.clone(),
                    offset,
                    length,
                    checksum: frame_checksum,
                },
            },
        );
    }

    if !content_pack.is_empty() {
        let final_path = root.join(&relative_path);
        let temporary_path = content_directory.join(format!(".{filename}.tmp"));
        write_synced(&temporary_path, &content_pack)?;
        if let Err(error) = fs::rename(&temporary_path, &final_path) {
            let _ = fs::remove_file(&temporary_path);
            return Err(error);
        }
        sync_directory(&content_directory)?;
    }

    if fault == PublishFault::AfterContentCommit {
        return Err(injected_publish_error("after content commit"));
    }

    let index = PackFileIndex {
        revision,
        guard,
        entries,
    };
    let index_bytes = encode_index(&index)?;
    let temporary_index = root.join(format!(".{INDEX_FILE}-{revision:016x}.tmp"));
    write_synced(&temporary_index, &index_bytes)?;
    if fault == PublishFault::BeforeIndexReplace {
        let _ = fs::remove_file(&temporary_index);
        return Err(injected_publish_error("before index replacement"));
    }
    if let Err(error) = fs::rename(&temporary_index, root.join(INDEX_FILE)) {
        let _ = fs::remove_file(&temporary_index);
        return Err(error);
    }
    sync_directory(root)
}

fn injected_publish_error(point: &str) -> io::Error {
    io::Error::other(format!("injected PackFile failure {point}"))
}

fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn sync_directory(path: &Path) -> io::Result<()> {
    fs::File::open(path)?.sync_all()
}

fn read_bounded(path: &Path, maximum: usize) -> Option<Vec<u8>> {
    let metadata = fs::metadata(path).ok()?;
    let length = usize::try_from(metadata.len()).ok()?;
    if length > maximum {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    (bytes.len() == length).then_some(bytes)
}

fn read_content_reference(path: &Path, reference: &ContentReference) -> Option<Vec<u8>> {
    let file_length = fs::metadata(path).ok()?.len();
    if usize::try_from(file_length).ok()? > MAX_CONTENT_BYTES {
        return None;
    }
    let end = reference.offset.checked_add(reference.length)?;
    if end > file_length {
        return None;
    }
    let length = usize::try_from(reference.length).ok()?;
    if length > MAX_CONTENT_BYTES {
        return None;
    }
    let mut file = fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(reference.offset)).ok()?;
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

fn encode_index(index: &PackFileIndex) -> io::Result<Vec<u8>> {
    let mut body = Encoder::default();
    body.write_u64(index.revision);
    match &index.guard {
        Some(guard) => {
            body.write_u8(1);
            body.write_bytes(&guard.version)?;
            encode_snapshot(&mut body, &guard.build_dependencies)?;
            encode_snapshot(&mut body, &guard.resolve_build_dependencies)?;
        }
        None => body.write_u8(0),
    }
    body.write_count(index.entries.len())?;
    for (address, entry) in &index.entries {
        body.write_bytes(&address.namespace)?;
        body.write_bytes(&address.identifier)?;
        body.write_optional_bytes(entry.etag.as_ref().map(|etag| etag.0.as_slice()))?;
        body.write_raw(entry.type_id.as_bytes());
        body.write_raw(&entry.codec_id.0);
        body.write_string(entry.content.file.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "content path is not UTF-8")
        })?)?;
        body.write_u64(entry.content.offset);
        body.write_u64(entry.content.length);
        body.write_u64(entry.content.checksum);
    }
    encode_frame(INDEX_MAGIC, &body.finish(), MAX_INDEX_BYTES)
}

fn decode_index(bytes: &[u8]) -> Option<PackFileIndex> {
    let body = decode_frame(bytes, INDEX_MAGIC, MAX_INDEX_BYTES)?;
    let mut decoder = Decoder::new(body);
    let revision = decoder.read_u64()?;
    let guard = match decoder.read_u8()? {
        0 => None,
        1 => Some(PackFileGuardDto {
            version: decoder.read_bytes()?,
            build_dependencies: decode_snapshot(&mut decoder)?,
            resolve_build_dependencies: decode_snapshot(&mut decoder)?,
        }),
        _ => return None,
    };
    let count = decoder.read_count()?;
    let mut entries = BTreeMap::new();
    for _ in 0..count {
        let address = PackFileAddress {
            namespace: decoder.read_bytes()?,
            identifier: decoder.read_bytes()?,
        };
        let etag = decoder.read_optional_bytes()?.map(PackFileETag);
        let type_id = StableTypeId(decoder.read_exact()?);
        let codec_id = StableCodecId(decoder.read_exact()?);
        let content_file = PathBuf::from(decoder.read_string()?);
        if !is_safe_relative_path(&content_file) {
            return None;
        }
        let offset = decoder.read_u64()?;
        let length = decoder.read_u64()?;
        if usize::try_from(length).ok()? > MAX_CONTENT_BYTES {
            return None;
        }
        let checksum = decoder.read_u64()?;
        entries.insert(
            address,
            PackFileIndexEntry {
                etag,
                type_id,
                codec_id,
                content: ContentReference {
                    file: content_file,
                    offset,
                    length,
                    checksum,
                },
            },
        );
    }
    decoder.finish()?;
    Some(PackFileIndex {
        revision,
        guard,
        entries,
    })
}

fn encode_content(
    type_id: StableTypeId,
    codec_id: StableCodecId,
    payload: &[u8],
) -> io::Result<Vec<u8>> {
    if payload.len() > MAX_RECORD_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "PackFile record exceeds the configured bound",
        ));
    }
    let mut body = Encoder::default();
    body.write_raw(type_id.as_bytes());
    body.write_raw(&codec_id.0);
    body.write_u64(u64::try_from(payload.len()).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidData, "PackFile payload is too large")
    })?);
    body.write_u64(checksum(payload));
    body.write_raw(payload);
    encode_frame(CONTENT_MAGIC, &body.finish(), MAX_CONTENT_BYTES)
}

fn decode_content(bytes: &[u8]) -> Option<(StableTypeId, StableCodecId, &[u8])> {
    let body = decode_frame(bytes, CONTENT_MAGIC, MAX_CONTENT_BYTES)?;
    let mut decoder = Decoder::new(body);
    let type_id = StableTypeId(decoder.read_exact()?);
    let codec_id = StableCodecId(decoder.read_exact()?);
    let length = usize::try_from(decoder.read_u64()?).ok()?;
    if length > MAX_RECORD_BYTES {
        return None;
    }
    let expected_checksum = decoder.read_u64()?;
    let start = decoder.position;
    let end = start.checked_add(length)?;
    let payload = body.get(start..end)?;
    decoder.position = end;
    decoder.finish()?;
    (checksum(payload) == expected_checksum).then_some((type_id, codec_id, payload))
}

fn encode_frame(magic: &[u8], body: &[u8], maximum: usize) -> io::Result<Vec<u8>> {
    let framed_length = magic
        .len()
        .checked_add(16)
        .and_then(|length| length.checked_add(body.len()))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "PackFile frame overflow"))?;
    if framed_length > maximum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "PackFile frame exceeds the configured bound",
        ));
    }
    let mut bytes = Vec::with_capacity(framed_length);
    bytes.extend_from_slice(magic);
    bytes.extend_from_slice(
        &u64::try_from(body.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame is too large"))?
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&checksum(body).to_le_bytes());
    bytes.extend_from_slice(body);
    Ok(bytes)
}

fn decode_frame<'a>(bytes: &'a [u8], magic: &[u8], maximum: usize) -> Option<&'a [u8]> {
    if bytes.len() > maximum || !bytes.starts_with(magic) {
        return None;
    }
    let header_end = magic.len().checked_add(16)?;
    let header = bytes.get(magic.len()..header_end)?;
    let length = usize::try_from(u64::from_le_bytes(header.get(..8)?.try_into().ok()?)).ok()?;
    let expected_checksum = u64::from_le_bytes(header.get(8..16)?.try_into().ok()?);
    let end = header_end.checked_add(length)?;
    if end != bytes.len() {
        return None;
    }
    let body = bytes.get(header_end..end)?;
    (checksum(body) == expected_checksum).then_some(body)
}

fn checksum(bytes: &[u8]) -> u64 {
    let mut state = 14_695_981_039_346_656_037_u64;
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(1_099_511_628_211);
    }
    state
}

fn is_safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}
