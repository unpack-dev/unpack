use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};

use futures::{FutureExt, StreamExt, future::BoxFuture, stream::FuturesUnordered};
use tokio::sync::{Mutex, Semaphore};

use crate::{
    CompilerOptions, Dependency, DependencyKind, Error, ModuleGraph, ModuleId, ModuleIdentity,
    NormalModuleFactory, Result, UnpackResolver,
    parser::{ParsedModule, parse_module_dependencies},
};

#[derive(Debug, Default)]
pub(crate) struct MakeState {
    pub module_graph: ModuleGraph,
    pub entries: BTreeMap<usize, ModuleId>,
    pub errors: Vec<Error>,
    modules_by_identity: HashMap<ModuleIdentity, ModuleId>,
}

#[derive(Debug, Clone)]
struct MakeServices {
    factory: NormalModuleFactory,
    semaphore: Arc<Semaphore>,
}

#[derive(Debug, Clone)]
struct MakeRequest {
    entry_index: Option<usize>,
    origin_module: Option<ModuleId>,
    origin_block: Option<usize>,
    context: PathBuf,
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
    state: Arc<Mutex<MakeState>>,
) -> Result<()> {
    let services = MakeServices {
        factory: NormalModuleFactory::new(resolver),
        semaphore: Arc::new(Semaphore::new(options.parallelism.max(1))),
    };

    let mut queue = FuturesUnordered::new();
    for (entry_index, entry) in options.entries.iter().enumerate() {
        queue.push(make_task(
            MakeRequest {
                entry_index: Some(entry_index),
                origin_module: None,
                origin_block: None,
                context: options.context.clone(),
                dependency: Dependency::new(DependencyKind::Entry, entry.request.clone()),
            },
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
    request: MakeRequest,
    services: MakeServices,
    state: Arc<Mutex<MakeState>>,
) -> BoxFuture<'static, Result<Vec<MakeRequest>>> {
    async move {
        let _permit = services
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("make semaphore should stay open");

        process_request(request, services, state).await
    }
    .boxed()
}

async fn process_request(
    request: MakeRequest,
    services: MakeServices,
    state: Arc<Mutex<MakeState>>,
) -> Result<Vec<MakeRequest>> {
    let factorized = match services
        .factory
        .factorize(&request.context, &request.dependency)
        .await
    {
        Ok(factorized) => factorized,
        Err(error) if error.is_compilation_error() => {
            let identity = failed_module_identity(&request.context, &request.dependency);
            let mut state = state.lock().await;
            let add_result = state.add_or_connect(
                request.origin_module,
                request.entry_index,
                request.origin_block,
                request.dependency,
                identity,
            );
            state.fail_module(add_result.module_id, error, String::new())?;
            return Ok(Vec::new());
        }
        Err(error) => return Err(error),
    };
    let identity = factorized.identity;
    let resource = factorized.resource;

    let add_result = {
        state.lock().await.add_or_connect(
            request.origin_module,
            request.entry_index,
            request.origin_block,
            request.dependency,
            identity,
        )
    };

    if !add_result.is_new {
        return Ok(Vec::new());
    }

    let source = match tokio::fs::read_to_string(&resource).await {
        Ok(source) => source,
        Err(error) => {
            let error = Error::read(&resource, error);
            state
                .lock()
                .await
                .fail_module(add_result.module_id, error, String::new())?;
            return Ok(Vec::new());
        }
    };
    let parsed = match parse_module_dependencies(resource.clone(), source.clone()).await {
        Ok(parsed) => parsed,
        Err(error) if error.is_compilation_error() => {
            state
                .lock()
                .await
                .fail_module(add_result.module_id, error, source)?;
            return Ok(Vec::new());
        }
        Err(error) => return Err(error),
    };
    let issuer_context = resource
        .parent()
        .ok_or(Error::MissingModuleDirectory(add_result.module_id))?
        .to_path_buf();

    let mut children = parsed
        .dependencies
        .iter()
        .filter(|dependency| dependency.is_module_dependency())
        .cloned()
        .map(|dependency| MakeRequest {
            entry_index: None,
            origin_module: Some(add_result.module_id),
            origin_block: None,
            context: issuer_context.clone(),
            dependency,
        })
        .collect::<Vec<_>>();

    for (block_index, block) in parsed.blocks.iter().enumerate() {
        children.extend(
            block
                .dependencies()
                .iter()
                .filter(|dependency| dependency.is_module_dependency())
                .cloned()
                .map(|dependency| MakeRequest {
                    entry_index: None,
                    origin_module: Some(add_result.module_id),
                    origin_block: Some(block_index),
                    context: issuer_context.clone(),
                    dependency,
                }),
        );
    }

    state
        .lock()
        .await
        .finish_build(add_result.module_id, parsed, source)?;

    Ok(children)
}

fn failed_module_identity(context: &Path, dependency: &Dependency) -> ModuleIdentity {
    let request = dependency.request().unwrap_or("<unknown>");
    let resource = if Path::new(request).is_absolute() {
        PathBuf::from(request)
    } else {
        context.join(request)
    };
    ModuleIdentity::new(resource)
}

impl MakeState {
    fn add_or_connect(
        &mut self,
        origin_module: Option<ModuleId>,
        entry_index: Option<usize>,
        origin_block: Option<usize>,
        dependency: Dependency,
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

        if let Some(entry_index) = entry_index {
            self.entries.insert(entry_index, module_id);
        }
        self.module_graph
            .connect(origin_module, origin_block, dependency, module_id);

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
