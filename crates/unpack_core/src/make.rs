use std::{collections::HashMap, path::PathBuf, sync::Arc};

use futures::{FutureExt, StreamExt, future::BoxFuture, stream::FuturesUnordered};
use tokio::sync::{Mutex, Semaphore};

use crate::{
    CompilerOptions, Dependency, DependencyKind, Error, ModuleGraph, ModuleId, ModuleIdentity,
    Result, UnpackResolver, parser::parse_static_esm_dependencies,
};

#[derive(Debug, Default)]
pub(crate) struct MakeState {
    pub module_graph: ModuleGraph,
    pub entries: Vec<ModuleId>,
    pub errors: Vec<Error>,
    modules_by_identity: HashMap<ModuleIdentity, ModuleId>,
}

#[derive(Debug, Clone)]
struct MakeServices {
    resolver: UnpackResolver,
    semaphore: Arc<Semaphore>,
}

#[derive(Debug, Clone)]
struct MakeRequest {
    origin_module: Option<ModuleId>,
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
        resolver,
        semaphore: Arc::new(Semaphore::new(options.parallelism.max(1))),
    };

    let mut queue = FuturesUnordered::new();
    for entry in &options.entries {
        queue.push(make_task(
            MakeRequest {
                origin_module: None,
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
    let resolved = services
        .resolver
        .resolve(&request.context, &request.dependency.request)
        .await?;
    let identity = ModuleIdentity::from(resolved);
    let resource = identity.resource.clone();

    let add_result = {
        state
            .lock()
            .await
            .add_or_connect(request.origin_module, request.dependency, identity)
    };

    if !add_result.is_new {
        return Ok(Vec::new());
    }

    let source = tokio::fs::read_to_string(&resource)
        .await
        .map_err(|error| Error::read(&resource, error))?;
    let source_len = source.len();
    let dependencies = parse_static_esm_dependencies(resource.clone(), source).await?;
    let issuer_context = resource
        .parent()
        .ok_or(Error::MissingModuleDirectory(add_result.module_id))?
        .to_path_buf();

    let children = dependencies
        .iter()
        .cloned()
        .map(|dependency| MakeRequest {
            origin_module: Some(add_result.module_id),
            context: issuer_context.clone(),
            dependency,
        })
        .collect();

    state
        .lock()
        .await
        .finish_build(add_result.module_id, dependencies, source_len)?;

    Ok(children)
}

impl MakeState {
    fn add_or_connect(
        &mut self,
        origin_module: Option<ModuleId>,
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

        if origin_module.is_none() && !self.entries.contains(&module_id) {
            self.entries.push(module_id);
        }
        self.module_graph
            .connect(origin_module, dependency, module_id);

        AddModuleResult { module_id, is_new }
    }

    fn finish_build(
        &mut self,
        module_id: ModuleId,
        dependencies: Vec<Dependency>,
        source_len: usize,
    ) -> Result<()> {
        let module = self
            .module_graph
            .module_mut(module_id)
            .ok_or(Error::MissingModule(module_id))?;
        module.finish_build(dependencies, source_len);
        Ok(())
    }
}
