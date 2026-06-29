use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};

use futures::{FutureExt, StreamExt, future::BoxFuture, stream::FuturesUnordered};
use tokio::sync::{Mutex, Semaphore};

use crate::{
    CompilerOptions, Dependency, DependencyKind, Error, FactorizedModule, ModuleGraph, ModuleId,
    ModuleIdentity, NormalModuleFactory, Result, SnapshotStrategy, UnpackResolver,
    build_cache::{BuildCache, ModuleBuildCache, ModuleBuildRecord},
    parser::{ParsedModule, parse_module_dependencies},
    snapshot::FileSnapshot,
};

#[derive(Debug, Default)]
pub(crate) struct MakeState {
    pub module_graph: ModuleGraph,
    pub entries: BTreeMap<usize, ModuleId>,
    pub errors: Vec<Error>,
    pub file_dependencies: BTreeSet<PathBuf>,
    pub context_dependencies: BTreeSet<PathBuf>,
    pub missing_dependencies: BTreeSet<PathBuf>,
    modules_by_identity: HashMap<ModuleIdentity, ModuleId>,
}

#[derive(Clone)]
struct MakeServices {
    factory: NormalModuleFactory,
    module_build_cache: ModuleBuildCache,
    module_snapshot_strategy: SnapshotStrategy,
    semaphore: Arc<Semaphore>,
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
    origin_module: Option<ModuleId>,
    context: PathBuf,
    dependencies: Vec<QueuedDependency>,
}

#[derive(Debug, Clone)]
struct AddTask {
    origin_module: Option<ModuleId>,
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
    module_id: ModuleId,
    identity: ModuleIdentity,
    resource: PathBuf,
}

#[derive(Debug, Clone)]
struct ProcessDependenciesTask {
    origin_module: ModuleId,
    context: PathBuf,
    dependencies: Vec<QueuedDependency>,
}

#[derive(Debug, Clone)]
struct QueuedDependency {
    entry_index: Option<usize>,
    origin_block: Option<usize>,
    dependency: Dependency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AddModuleResult {
    module_id: ModuleId,
    is_new: bool,
}

pub(crate) async fn run(
    options: &CompilerOptions,
    resolver: UnpackResolver,
    build_cache: BuildCache,
    state: Arc<Mutex<MakeState>>,
) -> Result<()> {
    let services = MakeServices {
        factory: NormalModuleFactory::new(
            resolver,
            build_cache.normal_module_factory(),
            options.snapshot.resolve,
        ),
        module_build_cache: build_cache.module_builds(),
        module_snapshot_strategy: options.snapshot.module,
        semaphore: Arc::new(Semaphore::new(options.parallelism.max(1))),
    };

    let mut queue = FuturesUnordered::new();
    for (entry_index, entry) in options.entries.iter().enumerate() {
        queue.push(make_task(
            MakeTask::Factorize(FactorizeTask {
                origin_module: None,
                context: options.context.clone(),
                dependencies: vec![QueuedDependency {
                    entry_index: Some(entry_index),
                    origin_block: None,
                    dependency: Dependency::new(DependencyKind::Entry, entry.request.clone()),
                }],
            }),
            services.clone(),
            Arc::clone(&state),
        ));
    }

    while let Some(result) = queue.next().await {
        match result {
            Ok(children) => {
                for child in children {
                    queue.push(make_task(child, services.clone(), Arc::clone(&state)));
                }
            }
            Err(error) => {
                state.lock().await.errors.push(error.clone());
                return Err(error);
            }
        }
    }

    Ok(())
}

fn make_task(
    task: MakeTask,
    services: MakeServices,
    state: Arc<Mutex<MakeState>>,
) -> BoxFuture<'static, Result<Vec<MakeTask>>> {
    async move { task.run(services, state).await }.boxed()
}

impl MakeTask {
    async fn run(
        self,
        services: MakeServices,
        state: Arc<Mutex<MakeState>>,
    ) -> Result<Vec<MakeTask>> {
        match self {
            Self::Factorize(task) => task.run(services).await,
            Self::Add(task) => task.run(state).await,
            Self::Build(task) => task.run(services, state).await,
            Self::ProcessDependencies(task) => Ok(task.run()),
        }
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
        let result = match services.factory.factorize(&self.context, &dependency).await {
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
                let add_result = {
                    let mut state = state.lock().await;
                    state
                        .file_dependencies
                        .extend(factorized.file_dependencies.iter().cloned());
                    state
                        .missing_dependencies
                        .extend(factorized.missing_dependencies.iter().cloned());
                    state.file_dependencies.insert(resource.clone());
                    state.add_or_connect(self.origin_module, self.dependencies, identity.clone())
                };

                if !add_result.is_new {
                    return Ok(Vec::new());
                }

                Ok(vec![MakeTask::Build(BuildTask {
                    module_id: add_result.module_id,
                    identity,
                    resource,
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
                    state.add_or_connect(self.origin_module, self.dependencies, identity);
                state.fail_module(add_result.module_id, error, String::new())?;
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
            .ok_or(Error::MissingModuleDirectory(self.module_id))?
            .to_path_buf();

        if let Some(record) = services.module_build_cache.get(&self.identity) {
            if record
                .is_valid(&self.resource, services.module_snapshot_strategy)
                .await
            {
                let process_dependencies =
                    process_dependencies_task(self.module_id, &issuer_context, record.parsed());
                let (parsed, source) = record.into_parts();
                state
                    .lock()
                    .await
                    .finish_build(self.module_id, parsed, source)?;
                return Ok(process_dependencies.into_iter().collect());
            }
        }

        let source = match tokio::fs::read_to_string(&self.resource).await {
            Ok(source) => source,
            Err(error) => {
                let error = Error::read(&self.resource, error);
                state
                    .lock()
                    .await
                    .fail_module(self.module_id, error, String::new())?;
                return Ok(Vec::new());
            }
        };
        let parsed = match parse_module_dependencies(self.resource.clone(), source.clone()).await {
            Ok(parsed) => parsed,
            Err(error) if error.is_compilation_error() => {
                state
                    .lock()
                    .await
                    .fail_module(self.module_id, error, source)?;
                return Ok(Vec::new());
            }
            Err(error) => return Err(error),
        };
        let process_dependencies =
            process_dependencies_task(self.module_id, &issuer_context, &parsed);
        let snapshot =
            FileSnapshot::create(&self.resource, &source, services.module_snapshot_strategy)
                .await?;
        let record = ModuleBuildRecord::new(parsed.clone(), source.clone(), snapshot);

        state
            .lock()
            .await
            .finish_build(self.module_id, parsed, source)?;
        services.module_build_cache.store(self.identity, record);

        Ok(process_dependencies.into_iter().collect())
    }
}

impl ProcessDependenciesTask {
    fn run(self) -> Vec<MakeTask> {
        group_dependencies_by_resource_identifier(self.dependencies)
            .into_iter()
            .map(|dependencies| {
                MakeTask::Factorize(FactorizeTask {
                    origin_module: Some(self.origin_module),
                    context: self.context.clone(),
                    dependencies,
                })
            })
            .collect()
    }
}

fn process_dependencies_task(
    module_id: ModuleId,
    issuer_context: &Path,
    parsed: &ParsedModule,
) -> Option<MakeTask> {
    let issuer_context = issuer_context.to_path_buf();
    let mut dependencies = parsed
        .dependencies
        .iter()
        .cloned()
        .map(|dependency| QueuedDependency {
            entry_index: None,
            origin_block: None,
            dependency,
        })
        .collect::<Vec<_>>();

    for (block_index, block) in parsed.blocks.iter().enumerate() {
        dependencies.extend(block.dependencies().iter().cloned().map(|dependency| {
            QueuedDependency {
                entry_index: None,
                origin_block: Some(block_index),
                dependency,
            }
        }));
    }

    if dependencies.is_empty() {
        None
    } else {
        Some(MakeTask::ProcessDependencies(ProcessDependenciesTask {
            origin_module: module_id,
            context: issuer_context,
            dependencies,
        }))
    }
}

fn group_dependencies_by_resource_identifier(
    dependencies: Vec<QueuedDependency>,
) -> Vec<Vec<QueuedDependency>> {
    let mut groups: Vec<(String, Vec<QueuedDependency>)> = Vec::new();

    for dependency in dependencies {
        let Some(resource_identifier) = dependency.dependency.resource_identifier() else {
            continue;
        };
        if let Some((_, group)) = groups
            .iter_mut()
            .find(|(key, _)| key == &resource_identifier)
        {
            group.push(dependency);
        } else {
            groups.push((resource_identifier, vec![dependency]));
        }
    }

    groups
        .into_iter()
        .map(|(_, dependencies)| dependencies)
        .collect()
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
        origin_module: Option<ModuleId>,
        dependencies: Vec<QueuedDependency>,
        identity: ModuleIdentity,
    ) -> AddModuleResult {
        let (module_id, is_new) =
            if let Some(module_id) = self.modules_by_identity.get(&identity).copied() {
                (module_id, false)
            } else {
                let module_id = self.module_graph.add_module(identity.clone());
                self.modules_by_identity.insert(identity, module_id);
                (module_id, true)
            };

        for dependency in dependencies {
            if let Some(entry_index) = dependency.entry_index {
                self.entries.insert(entry_index, module_id);
            }
            self.module_graph.connect(
                origin_module,
                dependency.origin_block,
                dependency.dependency,
                module_id,
            );
        }

        AddModuleResult { module_id, is_new }
    }

    fn finish_build(
        &mut self,
        module_id: ModuleId,
        parsed: ParsedModule,
        source: String,
    ) -> Result<()> {
        let module = self
            .module_graph
            .module_mut(module_id)
            .ok_or(Error::MissingModule(module_id))?;
        module.finish_build(
            parsed.dependencies,
            parsed.blocks,
            parsed.presentational_dependencies,
            source,
        );
        Ok(())
    }

    fn fail_module(&mut self, module_id: ModuleId, error: Error, source: String) -> Result<()> {
        let module = self
            .module_graph
            .module_mut(module_id)
            .ok_or(Error::MissingModule(module_id))?;
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
        let build_cache =
            BuildCache::new(options.cache.clone(), options.snapshot.build_dependencies);
        let state = Arc::new(Mutex::new(MakeState::default()));

        run(&options, resolver, build_cache.clone(), Arc::clone(&state)).await?;

        let cache = build_cache.stats();
        let state = state.lock().await;
        let graph = &state.module_graph;
        let dep = graph
            .modules()
            .iter()
            .find(|module| module.identity().resource.ends_with("dep.js"))
            .expect("dep module should exist")
            .id();

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
