// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/Compilation.js

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use futures::{StreamExt, stream::FuturesUnordered};
use tokio::{
    sync::{Mutex, Semaphore},
    task::JoinHandle,
};

use crate::{
    AsyncDependenciesBlockIndex, CompilerOptions, Dependency, DependencyIndex, DependencyKind,
    Error, FactorizedModule, LoaderRequest, LoaderRunner, MatchedLoader, ModuleGraph, ModuleHandle,
    ModuleIdentity, ModuleType, NormalModuleFactory, Result, SnapshotStrategy, UnpackResolver,
    cache::{BuildCache, ModuleBuildRecord},
    cache_facade::ModuleBuildCache,
    module::BuiltModuleContent,
    parser::{ParsedModule, parse_module_dependencies},
    snapshot::{FileSystemInfo, SnapshotCache},
};

#[derive(Debug, Default)]
pub(crate) struct MakeState {
    pub module_graph: ModuleGraph,
    pub entries: BTreeMap<usize, ModuleHandle>,
    pub errors: Vec<Error>,
    pub file_dependencies: HashSet<PathBuf>,
    pub context_dependencies: HashSet<PathBuf>,
    pub missing_dependencies: HashSet<PathBuf>,
    modules_by_identity: HashMap<ModuleIdentity, ModuleHandle>,
}

#[derive(Clone)]
struct MakeServices {
    normal_module_factory: NormalModuleFactory,
    module_build_cache: ModuleBuildCache,
    module_snapshot_strategy: SnapshotStrategy,
    file_system_info: FileSystemInfo,
    snapshot_cache: SnapshotCache,
    loader_runner: Option<Arc<dyn LoaderRunner>>,
    metrics: Arc<MakeMetrics>,
    semaphore: Arc<Semaphore>,
}

#[derive(Debug)]
struct MakeMetrics {
    enabled: bool,
    factorize_task_ns: AtomicU64,
    build_task_ns: AtomicU64,
}

#[derive(Debug, Clone, Copy)]
enum MakeTaskMetric {
    Factorize,
    Build,
}

impl MakeMetrics {
    fn new() -> Self {
        Self {
            enabled: tracing::enabled!(target: "unpack_core::make_work", tracing::Level::INFO),
            factorize_task_ns: AtomicU64::new(0),
            build_task_ns: AtomicU64::new(0),
        }
    }

    fn observe(&self, metric: &AtomicU64, started: Option<Instant>) {
        if let Some(started) = started {
            metric.fetch_add(
                started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
                Ordering::Relaxed,
            );
        }
    }

    fn observe_task(&self, metric: MakeTaskMetric, started: Option<Instant>) {
        let total = match metric {
            MakeTaskMetric::Factorize => &self.factorize_task_ns,
            MakeTaskMetric::Build => &self.build_task_ns,
        };
        self.observe(total, started);
    }

    fn started(&self) -> Option<Instant> {
        self.enabled.then(Instant::now)
    }

    fn emit(&self) {
        if !self.enabled {
            return;
        }
        tracing::info!(
            target: "unpack_core::make_work",
            factorize_task_ms = self.factorize_task_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            build_task_ms = self.build_task_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            "make_work"
        );
    }
}

#[derive(Debug, Clone)]
enum MakeTask {
    Factorize(FactorizeTask),
    Add(AddTask),
    Build(BuildTask),
    ProcessDependencies(ProcessDependenciesTask),
}

#[derive(Debug, Clone)]
struct FactorizeTask {
    factory: ModuleFactoryKind,
    origin_module: Option<ModuleHandle>,
    context: PathBuf,
    dependencies: Vec<QueuedDependency>,
}

#[derive(Debug, Clone)]
struct AddTask {
    origin_module: Option<ModuleHandle>,
    context: PathBuf,
    dependencies: Vec<QueuedDependency>,
    result: FactorizeTaskResult,
}

#[derive(Debug, Clone)]
enum FactorizeTaskResult {
    Success(FactorizedModule),
    Failed(Error),
}

#[derive(Debug, Clone)]
struct BuildTask {
    module_handle: ModuleHandle,
    identity: ModuleIdentity,
    resource: PathBuf,
    loader: Option<MatchedLoader>,
}

#[derive(Debug, Clone)]
struct ProcessDependenciesTask {
    origin_module: ModuleHandle,
    context: PathBuf,
    dependencies: Vec<QueuedDependency>,
}

#[derive(Debug, Clone)]
struct QueuedDependency {
    entry_index: Option<usize>,
    origin_block: Option<AsyncDependenciesBlockIndex>,
    origin_dependency_index: Option<DependencyIndex>,
    dependency: Dependency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AddModuleResult {
    module_handle: ModuleHandle,
    is_new: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ModuleFactoryKind {
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum DependencyCategory {
    Esm,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FactorizeGroupKey {
    factory: ModuleFactoryKind,
    category: DependencyCategory,
    resource_identifier: String,
}

type BackgroundMakeTask = JoinHandle<Result<Vec<MakeTask>>>;

pub(crate) async fn run(
    options: &CompilerOptions,
    resolver: UnpackResolver,
    build_cache: BuildCache,
    file_system_info: FileSystemInfo,
    state: Arc<Mutex<MakeState>>,
) -> Result<()> {
    let snapshot_cache = SnapshotCache::default();
    let services = MakeServices {
        normal_module_factory: NormalModuleFactory::new(
            resolver,
            build_cache.normal_module_factory(),
            file_system_info.clone(),
            options.snapshot.resolve,
            snapshot_cache.clone(),
        )
        .with_module_rules(options.module_rules.clone())
        .with_side_effects(options.side_effects != crate::SideEffectsOption::Disabled),
        module_build_cache: build_cache.module_builds(),
        file_system_info,
        module_snapshot_strategy: options.snapshot.module,
        snapshot_cache,
        loader_runner: options.loader_runner.clone(),
        metrics: Arc::new(MakeMetrics::new()),
        semaphore: Arc::new(Semaphore::new(options.parallelism.max(1))),
    };

    let mut main_queue = VecDeque::new();
    let mut background_queue = FuturesUnordered::new();
    for (entry_index, entry) in options.entries.iter().enumerate() {
        schedule_make_task(
            MakeTask::Factorize(FactorizeTask {
                factory: ModuleFactoryKind::Normal,
                origin_module: None,
                context: options.context.clone(),
                dependencies: vec![QueuedDependency {
                    entry_index: Some(entry_index),
                    origin_block: None,
                    origin_dependency_index: None,
                    dependency: Dependency::new(DependencyKind::Entry, entry.request.clone()),
                }],
            }),
            services.clone(),
            Arc::clone(&state),
            &mut main_queue,
            &mut background_queue,
        );
    }

    loop {
        while let Some(task) = main_queue.pop_front() {
            match task.run(services.clone(), Arc::clone(&state)).await {
                Ok(children) => {
                    for child in children {
                        schedule_make_task(
                            child,
                            services.clone(),
                            Arc::clone(&state),
                            &mut main_queue,
                            &mut background_queue,
                        );
                    }
                }
                Err(error) => {
                    state.lock().await.errors.push(error.clone());
                    return Err(error);
                }
            }
        }

        if background_queue.is_empty() {
            services.metrics.emit();
            return Ok(());
        }

        let children = match next_background_make_task(&mut background_queue).await {
            Ok(children) => children,
            Err(error) => {
                state.lock().await.errors.push(error.clone());
                return Err(error);
            }
        };
        for child in children {
            schedule_make_task(
                child,
                services.clone(),
                Arc::clone(&state),
                &mut main_queue,
                &mut background_queue,
            );
        }
    }
}

fn schedule_make_task(
    task: MakeTask,
    services: MakeServices,
    state: Arc<Mutex<MakeState>>,
    main_queue: &mut VecDeque<MakeTask>,
    background_queue: &mut FuturesUnordered<BackgroundMakeTask>,
) {
    if task.is_background() {
        background_queue.push(spawn_make_task(task, services, state));
    } else {
        main_queue.push_back(task);
    }
}

fn spawn_make_task(
    task: MakeTask,
    services: MakeServices,
    state: Arc<Mutex<MakeState>>,
) -> BackgroundMakeTask {
    tokio::spawn(async move { task.run(services, state).await })
}

async fn next_background_make_task(
    background_queue: &mut FuturesUnordered<BackgroundMakeTask>,
) -> Result<Vec<MakeTask>> {
    background_queue
        .next()
        .await
        .expect("background queue should not be empty")
        .map_err(|error| Error::MakeTask {
            message: error.to_string(),
        })?
}

impl MakeTask {
    fn is_background(&self) -> bool {
        matches!(self, Self::Factorize(_) | Self::Build(_))
    }

    async fn run(
        self,
        services: MakeServices,
        state: Arc<Mutex<MakeState>>,
    ) -> Result<Vec<MakeTask>> {
        let metrics = Arc::clone(&services.metrics);
        let metric = match &self {
            Self::Factorize(_) => Some(MakeTaskMetric::Factorize),
            Self::Build(_) => Some(MakeTaskMetric::Build),
            Self::Add(_) | Self::ProcessDependencies(_) => None,
        };
        let started = metric.and_then(|_| metrics.started());
        let result = match self {
            Self::Factorize(task) => task.run(services).await,
            Self::Add(task) => task.run(state).await,
            Self::Build(task) => task.run(services, state).await,
            Self::ProcessDependencies(task) => Ok(task.run()),
        };
        if let Some(metric) = metric {
            metrics.observe_task(metric, started);
        }
        result
    }
}

impl FactorizeTask {
    async fn run(self, services: MakeServices) -> Result<Vec<MakeTask>> {
        let _permit = services
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("make semaphore should stay open");

        let dependency = self
            .dependencies
            .first()
            .expect("factorize task should have at least one dependency")
            .dependency
            .clone();
        let result = match self
            .factory
            .factorize(&services, &self.context, &dependency)
            .await
        {
            Ok(factorized) => FactorizeTaskResult::Success(factorized),
            Err(error) if error.is_compilation_error() => FactorizeTaskResult::Failed(error),
            Err(error) => return Err(error),
        };

        Ok(vec![MakeTask::Add(AddTask {
            origin_module: self.origin_module,
            context: self.context,
            dependencies: self.dependencies,
            result,
        })])
    }
}

impl AddTask {
    async fn run(self, state: Arc<Mutex<MakeState>>) -> Result<Vec<MakeTask>> {
        match self.result {
            FactorizeTaskResult::Success(factorized) => {
                let identity = factorized.identity;
                let resource = factorized.resource;
                let loader = factorized.loader;
                let side_effect_free = factorized.side_effect_free;
                let add_result = {
                    let mut state = state.lock().await;
                    state
                        .file_dependencies
                        .extend(factorized.file_dependencies.iter().cloned());
                    state
                        .context_dependencies
                        .extend(factorized.context_dependencies.iter().cloned());
                    state
                        .missing_dependencies
                        .extend(factorized.missing_dependencies.iter().cloned());
                    state.add_or_connect(
                        self.origin_module,
                        self.dependencies,
                        identity.clone(),
                        side_effect_free,
                    )
                };

                if !add_result.is_new {
                    return Ok(Vec::new());
                }

                Ok(vec![MakeTask::Build(BuildTask {
                    module_handle: add_result.module_handle,
                    identity,
                    resource,
                    loader,
                })])
            }
            FactorizeTaskResult::Failed(error) => {
                let dependency = &self
                    .dependencies
                    .first()
                    .expect("failed factorize task should have at least one dependency")
                    .dependency;
                let identity = failed_module_identity(&self.context, dependency);
                let mut state = state.lock().await;
                state.missing_dependencies.insert(identity.resource.clone());
                let add_result =
                    state.add_or_connect(self.origin_module, self.dependencies, identity, None);
                state.fail_module(add_result.module_handle, error, String::new())?;
                Ok(Vec::new())
            }
        }
    }
}

impl BuildTask {
    async fn run(
        self,
        services: MakeServices,
        state: Arc<Mutex<MakeState>>,
    ) -> Result<Vec<MakeTask>> {
        let _permit = services
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("make semaphore should stay open");

        let issuer_context = self
            .resource
            .parent()
            .ok_or(Error::MissingModuleDirectory(self.module_handle))?
            .to_path_buf();

        if let Some(record) = services.module_build_cache.get(&self.identity, None) {
            let valid = if services.module_snapshot_strategy.hash {
                record
                    .is_valid_with_cache(
                        &services.file_system_info,
                        services.module_snapshot_strategy,
                        &services.snapshot_cache,
                    )
                    .await
            } else {
                services.file_system_info.is_snapshot_valid_sync_with_cache(
                    record.snapshot(),
                    services.module_snapshot_strategy,
                    &services.snapshot_cache,
                )
            };
            if valid {
                let process_dependencies =
                    process_dependencies_task(self.module_handle, &issuer_context, record.parsed());
                state
                    .lock()
                    .await
                    .finish_build(self.module_handle, Arc::clone(record.built_content()))?;
                return Ok(process_dependencies.into_iter().collect());
            }
        }

        let raw_bytes = match tokio::fs::read(&self.resource).await {
            Ok(source) => source,
            Err(error) => {
                let error = Error::read(&self.resource, error);
                state
                    .lock()
                    .await
                    .fail_module(self.module_handle, error, String::new())?;
                return Ok(Vec::new());
            }
        };
        let raw_source = match String::from_utf8(raw_bytes.clone()) {
            Ok(source) => source,
            Err(error) if self.identity.module_type.is_asset() => {
                String::from_utf8_lossy(error.as_bytes()).into_owned()
            }
            Err(error) => {
                let error = Error::Read {
                    path: self.resource.clone(),
                    message: error.to_string(),
                };
                state
                    .lock()
                    .await
                    .fail_module(self.module_handle, error, String::new())?;
                return Ok(Vec::new());
            }
        };
        let mut source = if let Some(loader) = self.loader.as_ref() {
            let Some(loader_runner) = services.loader_runner.as_ref() else {
                let error = Error::Loader {
                    loader: loader.loader.clone(),
                    path: self.resource.clone(),
                    message: "loader runner is unavailable".to_string(),
                };
                state
                    .lock()
                    .await
                    .fail_module(self.module_handle, error, raw_source)?;
                return Ok(Vec::new());
            };
            match loader_runner
                .run(LoaderRequest {
                    loader: loader.loader.clone(),
                    resource: self.resource.clone(),
                    source: raw_source.clone(),
                    options: loader.options.clone(),
                })
                .await
            {
                Ok(source) => source,
                Err(error) if error.is_compilation_error() => {
                    state
                        .lock()
                        .await
                        .fail_module(self.module_handle, error, raw_source)?;
                    return Ok(Vec::new());
                }
                Err(error) => return Err(error),
            }
        } else {
            raw_source.clone()
        };
        if self.identity.module_type == ModuleType::Json && source.starts_with('\u{feff}') {
            source.remove(0);
        }
        let parsed = match match self.identity.module_type {
            ModuleType::JavaScriptAuto => parse_module_dependencies(&self.resource, &source),
            ModuleType::Json => serde_json::from_str::<serde_json::Value>(&source)
                .map(|_| ParsedModule::default())
                .map_err(|error| Error::Parse {
                    path: self.resource.clone(),
                    message: error.to_string(),
                }),
            ModuleType::Asset
            | ModuleType::AssetResource
            | ModuleType::AssetInline
            | ModuleType::AssetSource => Ok(ParsedModule::default()),
        } {
            Ok(parsed) => parsed,
            Err(error) if error.is_compilation_error() => {
                state
                    .lock()
                    .await
                    .fail_module(self.module_handle, error, source)?;
                return Ok(Vec::new());
            }
            Err(error) => return Err(error),
        };
        let process_dependencies =
            process_dependencies_task(self.module_handle, &issuer_context, &parsed);

        let binary_source = self.identity.module_type.is_asset().then(|| {
            if self.loader.is_some() {
                source.as_bytes().to_vec()
            } else {
                raw_bytes.clone()
            }
        });
        if !services.module_build_cache.is_enabled() {
            let built_content = Arc::new(match binary_source {
                Some(binary_source) => {
                    BuiltModuleContent::new_binary(parsed, source, binary_source)
                }
                None => BuiltModuleContent::new(parsed, source),
            });
            state
                .lock()
                .await
                .finish_build(self.module_handle, built_content)?;
            return Ok(process_dependencies.into_iter().collect());
        }

        let mut snapshots = vec![
            services
                .file_system_info
                .create_file_snapshot_bytes(
                    &self.resource,
                    &raw_bytes,
                    services.module_snapshot_strategy,
                )
                .await?,
        ];
        if let Some(loader) = self.loader.as_ref() {
            let loader = &loader.loader;
            let loader_source = tokio::fs::read_to_string(loader)
                .await
                .map_err(|error| Error::read(loader, error))?;
            snapshots.push(
                services
                    .file_system_info
                    .create_file_snapshot(loader, &loader_source, services.module_snapshot_strategy)
                    .await?,
            );
        }
        let snapshot = services.file_system_info.merge_snapshots(snapshots.iter());
        let built_content = Arc::new(match binary_source {
            Some(binary_source) => BuiltModuleContent::new_binary(parsed, source, binary_source),
            None => BuiltModuleContent::new(parsed, source),
        });
        let record = ModuleBuildRecord::new(Arc::clone(&built_content), snapshot);

        state
            .lock()
            .await
            .finish_build(self.module_handle, built_content)?;
        services
            .module_build_cache
            .store(self.identity, None, record);

        Ok(process_dependencies.into_iter().collect())
    }
}

impl ProcessDependenciesTask {
    fn run(self) -> Vec<MakeTask> {
        group_dependencies_for_factorization(self.dependencies)
            .into_iter()
            .map(|(key, dependencies)| {
                MakeTask::Factorize(FactorizeTask {
                    factory: key.factory,
                    origin_module: Some(self.origin_module),
                    context: self.context.clone(),
                    dependencies,
                })
            })
            .collect()
    }
}

impl ModuleFactoryKind {
    async fn factorize(
        self,
        services: &MakeServices,
        context: &Path,
        dependency: &Dependency,
    ) -> Result<FactorizedModule> {
        match self {
            Self::Normal => {
                services
                    .normal_module_factory
                    .factorize(context, dependency)
                    .await
            }
        }
    }
}

impl FactorizeGroupKey {
    fn for_dependency(dependency: &Dependency) -> Option<Self> {
        Some(Self {
            factory: ModuleFactoryKind::for_dependency(dependency)?,
            category: DependencyCategory::for_dependency(dependency)?,
            resource_identifier: dependency.resource_identifier()?,
        })
    }
}

impl ModuleFactoryKind {
    fn for_dependency(dependency: &Dependency) -> Option<Self> {
        if dependency.is_module_dependency() {
            Some(Self::Normal)
        } else {
            None
        }
    }
}

impl DependencyCategory {
    fn for_dependency(dependency: &Dependency) -> Option<Self> {
        if dependency.is_module_dependency() {
            Some(Self::Esm)
        } else {
            None
        }
    }
}

fn process_dependencies_task(
    module_handle: ModuleHandle,
    issuer_context: &Path,
    parsed: &ParsedModule,
) -> Option<MakeTask> {
    let issuer_context = issuer_context.to_path_buf();
    let mut dependencies = parsed
        .dependencies_block
        .dependencies()
        .iter()
        .cloned()
        .enumerate()
        .map(|(dependency_index, dependency)| QueuedDependency {
            entry_index: None,
            origin_block: None,
            origin_dependency_index: Some(DependencyIndex::new(dependency_index)),
            dependency,
        })
        .collect::<Vec<_>>();

    for (block_index, block) in parsed.dependencies_block.blocks().iter().enumerate() {
        dependencies.extend(block.dependencies().iter().cloned().enumerate().map(
            |(dependency_index, dependency)| QueuedDependency {
                entry_index: None,
                origin_block: Some(AsyncDependenciesBlockIndex::new(block_index)),
                origin_dependency_index: Some(DependencyIndex::new(dependency_index)),
                dependency,
            },
        ));
    }

    if dependencies.is_empty() {
        None
    } else {
        Some(MakeTask::ProcessDependencies(ProcessDependenciesTask {
            origin_module: module_handle,
            context: issuer_context,
            dependencies,
        }))
    }
}

fn group_dependencies_for_factorization(
    dependencies: Vec<QueuedDependency>,
) -> Vec<(FactorizeGroupKey, Vec<QueuedDependency>)> {
    let mut groups: Vec<(FactorizeGroupKey, Vec<QueuedDependency>)> = Vec::new();

    for dependency in dependencies {
        let Some(key) = FactorizeGroupKey::for_dependency(&dependency.dependency) else {
            continue;
        };
        if let Some((_, group)) = groups.iter_mut().find(|(group_key, _)| group_key == &key) {
            group.push(dependency);
        } else {
            groups.push((key, vec![dependency]));
        }
    }

    groups
}

fn failed_module_identity(context: &Path, dependency: &Dependency) -> ModuleIdentity {
    let request = dependency.request().unwrap_or("<unknown>");
    let resource = if Path::new(request).is_absolute() {
        PathBuf::from(request)
    } else {
        context.join(request)
    };
    ModuleIdentity::new(normalize_missing_resource(resource))
}

fn normalize_missing_resource(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir if normalized.pop() => {}
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

impl MakeState {
    fn add_or_connect(
        &mut self,
        origin_module: Option<ModuleHandle>,
        dependencies: Vec<QueuedDependency>,
        identity: ModuleIdentity,
        side_effect_free: Option<bool>,
    ) -> AddModuleResult {
        let (module_handle, is_new) =
            if let Some(module_handle) = self.modules_by_identity.get(&identity).copied() {
                (module_handle, false)
            } else {
                let module_handle = self.module_graph.add_module(identity.clone());
                if let Some(module) = self.module_graph.module_mut(module_handle) {
                    module.set_factory_side_effect_free(side_effect_free);
                }
                self.modules_by_identity.insert(identity, module_handle);
                (module_handle, true)
            };

        for dependency in dependencies {
            if let Some(entry_index) = dependency.entry_index {
                self.entries.insert(entry_index, module_handle);
            }
            self.module_graph.connect(
                origin_module,
                dependency.origin_block,
                dependency.origin_dependency_index,
                dependency.dependency,
                module_handle,
            );
        }

        AddModuleResult {
            module_handle,
            is_new,
        }
    }

    fn finish_build(
        &mut self,
        module_handle: ModuleHandle,
        built_content: Arc<BuiltModuleContent>,
    ) -> Result<()> {
        let module = self
            .module_graph
            .module_mut(module_handle)
            .ok_or(Error::MissingModule(module_handle))?;
        module.finish_build_content(built_content);
        Ok(())
    }

    fn fail_module(
        &mut self,
        module_handle: ModuleHandle,
        error: Error,
        source: String,
    ) -> Result<()> {
        let module = self
            .module_graph
            .module_mut(module_handle)
            .ok_or(Error::MissingModule(module_handle))?;
        module.fail_build(error.clone(), source);
        self.errors.push(error);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, sync::Arc};

    use super::*;
    use crate::{Entry, UnpackResolver};

    #[tokio::test]
    async fn process_dependencies_groups_factorization_by_resource_identifier()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        write(
            temp.path().join("index.js"),
            r#"
                import { value } from "./dep";
                export { value as reexport } from "./dep";
                export const result = value;
            "#,
        )?;
        write(temp.path().join("dep.js"), "export const value = 1;")?;

        let options = CompilerOptions::new(temp.path(), vec![Entry::new("main", "./index")]);
        let resolver = UnpackResolver::new(options.resolve.clone());
        let build_cache = BuildCache::new(options.cache.clone(), options.snapshot.clone());
        let state = Arc::new(Mutex::new(MakeState::default()));

        run(
            &options,
            resolver,
            build_cache.clone(),
            FileSystemInfo::new(),
            Arc::clone(&state),
        )
        .await?;

        let cache = build_cache.stats();
        let state = state.lock().await;
        let graph = &state.module_graph;
        let dep = graph
            .modules()
            .iter()
            .find(|module| module.identity().resource.ends_with("dep.js"))
            .expect("dep module should exist")
            .handle();

        assert_eq!(state.errors, []);
        assert_eq!(graph.modules().len(), 2);
        assert_eq!(graph.incoming_connections(dep).count(), 4);
        assert_eq!(cache.resolve_entries, 2);
        assert_eq!(cache.resolve_misses, 2);
        assert_eq!(cache.resolve_hits, 0);

        Ok(())
    }

    fn write(path: impl AsRef<Path>, source: &str) -> std::io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, source)
    }
}
