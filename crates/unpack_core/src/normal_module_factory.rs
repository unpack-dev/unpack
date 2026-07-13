// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/NormalModuleFactory.js

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use dashmap::DashMap;
use tokio::sync::OnceCell;

use crate::{
    Dependency, Error, MatchedLoader, Module, ModuleGraph, ModuleHandle, ModuleIdentity,
    ModuleRule, ModuleType, Result, SnapshotStrategy, UnpackResolver,
    cache::{ResolveRecord, ResolveRequest},
    cache_facade::NormalModuleFactoryCache,
    code_generation_record::CodeGenerationRecord,
    id_assignment::RenderId,
    parser::ParsedModule,
    snapshot::{FileSystemInfo, SnapshotCache},
};

pub(crate) type ModuleParser = for<'a> fn(ModuleParserContext<'a>) -> Result<ParsedModule>;
pub(crate) type ModuleGenerator =
    for<'a> fn(ModuleGeneratorContext<'a>) -> Result<CodeGenerationRecord>;
type DefaultModuleTypeRule = fn(&Path) -> bool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModuleSourceKind {
    Text,
    Binary,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ModuleTypeRegistration {
    pub parser: ModuleParser,
    pub generator: ModuleGenerator,
    pub source_kind: ModuleSourceKind,
    pub side_effect_free: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct ModuleParserContext<'a> {
    pub module_type: ModuleType,
    pub resource: &'a Path,
    pub source: &'a str,
    pub source_bytes: &'a [u8],
    pub javascript_parser_hooks: &'a crate::parser::JavascriptParserHookSet,
}

#[derive(Clone, Copy)]
pub(crate) struct ModuleGeneratorContext<'a> {
    pub module: &'a Module,
    pub module_graph: &'a ModuleGraph,
    pub chunk_graph: &'a crate::ChunkGraph,
    pub module_render_ids: &'a HashMap<ModuleHandle, RenderId>,
}

#[derive(Clone, Default)]
pub(crate) struct ModuleTypeRegistry {
    registrations: BTreeMap<ModuleType, ModuleTypeRegistration>,
    default_rules: Vec<(ModuleType, DefaultModuleTypeRule)>,
    default_module_type: Option<ModuleType>,
}

impl ModuleTypeRegistry {
    pub(crate) fn register(
        &mut self,
        module_type: ModuleType,
        registration: ModuleTypeRegistration,
    ) {
        assert!(
            self.registrations
                .insert(module_type, registration)
                .is_none(),
            "module type {module_type:?} must only be registered once"
        );
    }

    pub(crate) fn register_default_rule(
        &mut self,
        module_type: ModuleType,
        matches: DefaultModuleTypeRule,
    ) {
        self.default_rules.push((module_type, matches));
    }

    pub(crate) fn set_default_module_type(&mut self, module_type: ModuleType) {
        assert!(
            self.default_module_type.replace(module_type).is_none(),
            "default module type must only be registered once"
        );
    }

    pub(crate) fn module_type_for_resource(&self, resource: &Path) -> Result<ModuleType> {
        self.default_rules
            .iter()
            .find_map(|(module_type, matches)| matches(resource).then_some(*module_type))
            .or(self.default_module_type)
            .ok_or_else(|| Error::ModuleTypeRegistry {
                message: format!(
                    "no plugin registered a default module type for {}",
                    resource.display()
                ),
            })
    }

    pub(crate) fn registration(&self, module_type: ModuleType) -> Result<ModuleTypeRegistration> {
        self.registrations
            .get(&module_type)
            .copied()
            .ok_or_else(|| Error::ModuleTypeRegistry {
                message: format!("no plugin registered module type {module_type:?}"),
            })
    }

    pub(crate) fn parse(&self, context: ModuleParserContext<'_>) -> Result<ParsedModule> {
        (self.registration(context.module_type)?.parser)(context)
    }

    pub(crate) fn generate(
        &self,
        context: ModuleGeneratorContext<'_>,
    ) -> Result<CodeGenerationRecord> {
        (self
            .registration(context.module.identity().module_type)?
            .generator)(context)
    }
}

impl std::fmt::Debug for ModuleTypeRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ModuleTypeRegistry")
            .field("registrations", &self.registrations.keys())
            .field("default_rules", &self.default_rules.len())
            .field("default_module_type", &self.default_module_type)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct NormalModuleFactory {
    resolver: UnpackResolver,
    cache: NormalModuleFactoryCache,
    file_system_info: FileSystemInfo,
    resolve_snapshot_strategy: SnapshotStrategy,
    runtime_factorize_cache: RuntimeFactorizeCache,
    package_side_effects_cache: PackageSideEffectsCache,
    snapshot_cache: SnapshotCache,
    module_types: ModuleTypeRegistry,
    module_rules: Vec<ModuleRule>,
    side_effects: bool,
    unsafe_watch_cache: Option<crate::unsafe_watch_cache::UnsafeWatchCache>,
    watch_change_set: Option<crate::WatchChangeSet>,
}

// Per-compilation singleflight cache; separate from Cache so cache:false
// still coalesces duplicate factory work within one make run.
type RuntimeFactorizeCache = Arc<DashMap<ResolveRequest, Arc<OnceCell<Result<FactorizedModule>>>>>;
type PackageSideEffectsCache = Arc<DashMap<PathBuf, Option<Arc<PackageSideEffects>>>>;

#[derive(Debug)]
struct PackageSideEffects {
    package_json: PathBuf,
    root: PathBuf,
    value: serde_json::Value,
}

impl NormalModuleFactory {
    pub(crate) fn new(
        resolver: UnpackResolver,
        cache: NormalModuleFactoryCache,
        file_system_info: FileSystemInfo,
        resolve_snapshot_strategy: SnapshotStrategy,
        snapshot_cache: SnapshotCache,
        module_types: ModuleTypeRegistry,
    ) -> Self {
        Self {
            resolver,
            cache,
            file_system_info,
            resolve_snapshot_strategy,
            runtime_factorize_cache: Arc::new(DashMap::new()),
            package_side_effects_cache: Arc::new(DashMap::new()),
            snapshot_cache,
            module_types,
            module_rules: Vec::new(),
            side_effects: false,
            unsafe_watch_cache: None,
            watch_change_set: None,
        }
    }

    pub(crate) fn with_module_rules(mut self, module_rules: Vec<ModuleRule>) -> Self {
        self.module_rules = module_rules;
        self
    }

    pub(crate) fn with_side_effects(mut self, side_effects: bool) -> Self {
        self.side_effects = side_effects;
        self
    }

    pub(crate) fn with_unsafe_watch_cache(
        mut self,
        cache: Option<crate::unsafe_watch_cache::UnsafeWatchCache>,
        change_set: Option<crate::WatchChangeSet>,
    ) -> Self {
        self.unsafe_watch_cache = cache;
        self.watch_change_set = change_set;
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
        let mut skip_ordinary_cache = false;
        if let (Some(cache), Some(changes)) = (&self.unsafe_watch_cache, &self.watch_change_set) {
            match cache.get_resolve(&resolve_request, changes) {
                crate::unsafe_watch_cache::UnsafeWatchCacheLookup::Reusable(record) => {
                    return self.apply_module_rules(
                        self.apply_factory_metadata(FactorizedModule::from_resolve_record(
                            &record,
                        ))?,
                    );
                }
                crate::unsafe_watch_cache::UnsafeWatchCacheLookup::Invalidated => {
                    skip_ordinary_cache = true;
                }
                crate::unsafe_watch_cache::UnsafeWatchCacheLookup::Miss => {}
            }
        }
        if !skip_ordinary_cache && let Some(record) = self.cache.get(&resolve_request, None) {
            let valid = if self.resolve_snapshot_strategy.hash {
                record
                    .is_valid_with_cache(
                        &self.file_system_info,
                        self.resolve_snapshot_strategy,
                        &self.snapshot_cache,
                    )
                    .await
            } else {
                record.is_valid_sync_with_cache(
                    &self.file_system_info,
                    self.resolve_snapshot_strategy,
                    &self.snapshot_cache,
                )
            };
            if valid {
                if let Some(cache) = &self.unsafe_watch_cache {
                    cache.remember_resolve(resolve_request, Arc::clone(&record));
                }
                return self.apply_module_rules(
                    self.apply_factory_metadata(FactorizedModule::from_resolve_record(&record))?,
                );
            }
        }

        self.factorize_with_runtime_cache(context, request, resolve_request)
            .await
            .and_then(|factorized| self.apply_factory_metadata(factorized))
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
                side_effect_free: None,
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
        if let Some(cache) = &self.unsafe_watch_cache {
            cache.remember_resolve(resolve_request.clone(), Arc::new(record.clone()));
        }
        self.cache.store(resolve_request, None, record);

        Ok(factorized)
    }

    fn apply_module_rules(&self, mut factorized: FactorizedModule) -> Result<FactorizedModule> {
        factorized.identity.module_type = self
            .module_types
            .module_type_for_resource(&factorized.resource)?;
        let mut matching = self
            .module_rules
            .iter()
            .filter(|rule| rule.matches(&factorized.resource));
        let Some(rule) = matching.next() else {
            return self.finalize_module_type_defaults(factorized);
        };
        if matching.next().is_some() {
            return Err(Error::LoaderRules {
                path: factorized.resource,
                message: "multiple matching rules would require a loader chain".to_string(),
            });
        }

        if let Some(module_type) = rule.module_type() {
            factorized.identity.module_type = module_type;
        }
        if let Some(loader) = rule.matched_loader() {
            factorized.identity.loaders = vec![loader.identifier.clone()];
            factorized.file_dependencies.insert(loader.loader.clone());
            factorized.loader = Some(loader);
        }
        if let Some(has_side_effects) = rule.side_effects() {
            factorized.side_effect_free = Some(!has_side_effects);
        }
        self.finalize_module_type_defaults(factorized)
    }

    fn finalize_module_type_defaults(
        &self,
        mut factorized: FactorizedModule,
    ) -> Result<FactorizedModule> {
        let registration = self
            .module_types
            .registration(factorized.identity.module_type)?;
        if factorized.side_effect_free.is_none() && registration.side_effect_free {
            factorized.side_effect_free = Some(true);
        }
        Ok(factorized)
    }

    fn apply_factory_metadata(&self, mut factorized: FactorizedModule) -> Result<FactorizedModule> {
        if !self.side_effects {
            return Ok(factorized);
        }
        let Some((package_json, relative_path, side_effects)) =
            package_side_effects(&factorized.resource, &self.package_side_effects_cache)?
        else {
            return Ok(factorized);
        };
        factorized.file_dependencies.insert(package_json);
        factorized.side_effect_free = Some(
            !crate::optimize::side_effects_flag_plugin::SideEffectsFlagPlugin::module_has_side_effects(
                &relative_path,
                &side_effects,
            ),
        );
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
    pub side_effect_free: Option<bool>,
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
            side_effect_free: None,
        }
    }
}

fn package_side_effects(
    resource: &Path,
    cache: &PackageSideEffectsCache,
) -> Result<Option<(PathBuf, String, serde_json::Value)>> {
    let mut directory = resource.parent();
    let mut uncached_directories = Vec::new();
    let metadata = loop {
        let Some(current) = directory else {
            break None;
        };
        if let Some(cached) = cache.get(current) {
            break cached.clone();
        }
        uncached_directories.push(current.to_path_buf());

        let package_json = current.join("package.json");
        if package_json.is_file() {
            let source = std::fs::read_to_string(&package_json)
                .map_err(|error| Error::read(&package_json, error))?;
            let data = serde_json::from_str::<serde_json::Value>(&source).map_err(|error| {
                Error::Resolve {
                    request: resource.display().to_string(),
                    issuer: current.to_path_buf(),
                    message: format!("invalid package.json: {error}"),
                }
            })?;
            break data.get("sideEffects").cloned().map(|value| {
                Arc::new(PackageSideEffects {
                    package_json,
                    root: current.to_path_buf(),
                    value,
                })
            });
        }
        directory = current.parent();
    };

    for directory in uncached_directories {
        cache.insert(directory, metadata.clone());
    }

    let Some(metadata) = metadata else {
        return Ok(None);
    };
    let relative_path = resource
        .strip_prefix(&metadata.root)
        .unwrap_or(resource)
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    Ok(Some((
        metadata.package_json.clone(),
        format!("./{relative_path}"),
        metadata.value.clone(),
    )))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::tempdir;

    use super::*;
    use crate::{
        CacheOptions, Dependency, HarmonyImportSideEffectDependency, UnpackResolver, cache::Cache,
        resolver::ResolveOptions,
    };

    #[test]
    fn package_side_effects_cache_reuses_a_shared_package_boundary()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let source_dir = temp.path().join("src");
        fs::create_dir(&source_dir)?;
        fs::write(
            temp.path().join("package.json"),
            r#"{"sideEffects":["./src/*.js"]}"#,
        )?;
        let first = source_dir.join("first.js");
        let second = source_dir.join("second.js");
        let cache = PackageSideEffectsCache::default();
        let side_effects = serde_json::json!(["./src/*.js"]);

        assert_eq!(
            package_side_effects(&first, &cache)?,
            Some((
                temp.path().join("package.json"),
                "./src/first.js".to_string(),
                side_effects.clone(),
            ))
        );
        assert_eq!(
            package_side_effects(&second, &cache)?,
            Some((
                temp.path().join("package.json"),
                "./src/second.js".to_string(),
                side_effects,
            ))
        );
        assert_eq!(cache.len(), 2);

        Ok(())
    }

    #[test]
    fn registry_rejects_unregistered_module_types() {
        let error = ModuleTypeRegistry::default()
            .registration(ModuleType::Json)
            .expect_err("unregistered module type should fail");
        assert!(
            error
                .to_string()
                .contains("no plugin registered module type Json")
        );
    }

    #[test]
    fn builtin_plugins_register_all_module_types_and_defaults() {
        let registry = crate::compiler::test_compilation_hooks().normal_module_factory_hooks;

        assert_eq!(
            registry
                .module_type_for_resource(Path::new("data.JSON"))
                .expect("JSON plugin should register its default rule"),
            ModuleType::Json
        );
        assert_eq!(
            registry
                .module_type_for_resource(Path::new("index.js"))
                .expect("JavaScript plugin should register the fallback"),
            ModuleType::JavaScriptAuto
        );
        for module_type in [ModuleType::JavaScriptAuto, ModuleType::Json] {
            assert_eq!(
                registry
                    .registration(module_type)
                    .expect("text module type should be registered")
                    .source_kind,
                ModuleSourceKind::Text
            );
        }
        for module_type in [
            ModuleType::Asset,
            ModuleType::AssetResource,
            ModuleType::AssetInline,
            ModuleType::AssetSource,
        ] {
            assert_eq!(
                registry
                    .registration(module_type)
                    .expect("asset module type should be registered")
                    .source_kind,
                ModuleSourceKind::Binary
            );
        }
    }

    #[tokio::test]
    async fn runtime_factorize_cache_reuses_results_when_build_cache_is_disabled()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        fs::write(temp.path().join("dep.js"), "export const value = 1;")?;

        let mut resolve_options = ResolveOptions::default();
        resolve_options.extensions = vec![".js".to_string()];
        let cache = Cache::new(CacheOptions::disabled(), crate::SnapshotOptions::default());
        let factory = NormalModuleFactory::new(
            UnpackResolver::new(resolve_options),
            cache.normal_module_factory(),
            FileSystemInfo::new(),
            SnapshotStrategy::timestamp(),
            SnapshotCache::default(),
            crate::compiler::test_compilation_hooks().normal_module_factory_hooks,
        );
        let dependency = Dependency::HarmonyImportSideEffect(
            HarmonyImportSideEffectDependency::new("./dep", 0, None),
        );

        let first = factory.factorize(temp.path(), &dependency).await?;
        let second = factory.factorize(temp.path(), &dependency).await?;

        assert_eq!(first, second);
        assert_eq!(cache.stats().resolve_entries, 0);
        assert_eq!(factory.runtime_factorize_cache.len(), 1);

        Ok(())
    }
}
