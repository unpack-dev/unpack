// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/webpack.js

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use napi::{
    Result, Status,
    bindgen_prelude::{Buffer, Either, FnArgs, Function, Promise},
    threadsafe_function::ThreadsafeFunction,
};
use napi_derive::napi;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use unpack_core::{
    Asset, BuildDependency, CacheCompression, CacheIdleReason, CacheOptions, CompilationHooks,
    Compiler, CompilerOptions, Dependency, Entry, Error as CoreError, HookFuture,
    InfrastructureLogEvent, InfrastructureLogLevel, InfrastructureLoggingOptions, LoaderFuture,
    LoaderRequest, LoaderRunner, Module, ModuleRule, ModuleType, SnapshotOptions,
    SnapshotPathPattern, SnapshotStrategy,
};

#[global_allocator]
#[cfg(not(any(miri, target_family = "wasm")))]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const MAX_BLOCKING_THREADS: usize = 4;
const INTERNAL_TRACING_ENV: &str = "UNPACK_INTERNAL_TRACING";
const DEFAULT_INTERNAL_TRACING_FILTER: &str = "unpack_core=trace,unpack_node=trace";

#[cfg(not(target_family = "wasm"))]
napi::ctor::declarative::ctor! {
    #[ctor(unsafe)]
    fn init_tokio_runtime() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .max_blocking_threads(MAX_BLOCKING_THREADS)
            .thread_name_fn(|| {
                static THREAD_ID: AtomicUsize = AtomicUsize::new(0);
                let id = THREAD_ID.fetch_add(1, Ordering::SeqCst);
                format!("unpack-tokio-{id}")
            })
            .enable_all()
            .build()
            .expect("create unpack tokio runtime failed");
        napi::bindgen_prelude::create_custom_tokio_runtime(runtime);
    }
}

#[napi(object)]
pub struct NativeEntry {
    pub name: String,
    pub request: String,
}

#[napi(object)]
pub struct NativeCompilerOptions {
    pub context: String,
    pub entries: Vec<NativeEntry>,
    #[napi(js_name = "outputPath")]
    pub output_path: String,
    pub cache: NativeCacheOptions,
    #[napi(js_name = "resolveCache")]
    pub resolve_cache: bool,
    pub snapshot: NativeSnapshotOptions,
    #[napi(js_name = "infrastructureLogging")]
    pub infrastructure_logging: NativeInfrastructureLoggingOptions,
    pub sourcemap: bool,
    #[napi(js_name = "providedExports")]
    pub provided_exports: bool,
    #[napi(js_name = "usedExports")]
    pub used_exports: bool,
    #[napi(js_name = "sideEffects")]
    pub side_effects: String,
    #[napi(js_name = "moduleRules")]
    pub module_rules: Vec<NativeModuleRule>,
    #[napi(js_name = "serialRebuildMake")]
    pub serial_rebuild_make: bool,
    #[napi(js_name = "unsafeWatchCacheInvalidation")]
    pub unsafe_watch_cache_invalidation: bool,
}

#[napi(object)]
pub struct NativeModuleRule {
    pub test: String,
    pub loader: Option<String>,
    #[napi(js_name = "type")]
    pub module_type: Option<String>,
    pub options: String,
    #[napi(js_name = "sideEffects")]
    pub side_effects: Option<bool>,
}

#[napi(object)]
pub struct NativeCacheOptions {
    #[napi(js_name = "type")]
    pub cache_type: String,
    #[napi(js_name = "cacheDirectory")]
    pub cache_directory: Option<String>,
    #[napi(js_name = "cacheLocation")]
    pub cache_location: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    #[napi(js_name = "buildDependencies")]
    pub build_dependencies: Vec<NativeBuildDependency>,
    #[napi(js_name = "automaticBuildDependencies")]
    pub automatic_build_dependencies: Vec<String>,
    #[napi(js_name = "maxAge")]
    pub max_age: Option<f64>,
    pub compression: Option<String>,
    #[napi(js_name = "allowCollectingMemory")]
    pub allow_collecting_memory: Option<bool>,
    #[napi(js_name = "maxMemoryGenerations")]
    pub max_memory_generations: Option<f64>,
    #[napi(js_name = "cacheUnaffected")]
    pub cache_unaffected: Option<bool>,
    #[napi(js_name = "memoryCacheUnaffected")]
    pub memory_cache_unaffected: Option<bool>,
    #[napi(js_name = "idleTimeout")]
    pub idle_timeout: Option<u32>,
    #[napi(js_name = "idleTimeoutForInitialStore")]
    pub idle_timeout_for_initial_store: Option<u32>,
    #[napi(js_name = "idleTimeoutAfterLargeChanges")]
    pub idle_timeout_after_large_changes: Option<u32>,
    pub profile: Option<bool>,
    #[napi(js_name = "readonly")]
    pub readonly: Option<bool>,
}

#[napi(object)]
pub struct NativeBuildDependency {
    pub name: String,
    pub requests: Vec<String>,
}

#[napi(object)]
pub struct NativeSnapshotOptions {
    pub module: NativeSnapshotStrategy,
    pub resolve: NativeSnapshotStrategy,
    #[napi(js_name = "buildDependencies")]
    pub build_dependencies: NativeSnapshotStrategy,
    #[napi(js_name = "resolveBuildDependencies")]
    pub resolve_build_dependencies: NativeSnapshotStrategy,
    #[napi(js_name = "managedPaths")]
    pub managed_paths: Vec<NativeSnapshotPathPattern>,
    #[napi(js_name = "immutablePaths")]
    pub immutable_paths: Vec<NativeSnapshotPathPattern>,
    #[napi(js_name = "unmanagedPaths")]
    pub unmanaged_paths: Vec<NativeSnapshotPathPattern>,
}

#[napi(object)]
pub struct NativeSnapshotStrategy {
    pub timestamp: bool,
    pub hash: bool,
}

#[napi(object)]
pub struct NativeSnapshotPathPattern {
    #[napi(js_name = "type")]
    pub pattern_type: String,
    pub path: Option<String>,
    pub source: Option<String>,
    pub flags: Option<String>,
}

#[napi(object)]
pub struct NativeInfrastructureLoggingOptions {
    pub level: String,
}

#[napi(object)]
pub struct NativeStatsError {
    pub message: String,
    pub path: Option<String>,
    pub request: Option<String>,
    pub issuer: Option<String>,
    pub stack: Option<String>,
}

#[napi(object)]
pub struct NativeAsset {
    pub name: String,
    pub size: u32,
}

#[napi(object)]
pub struct NativeAssetSource {
    pub name: String,
    pub source: Buffer,
}

#[napi(object)]
pub struct NativeStatsJson {
    pub errors: Vec<NativeStatsError>,
    pub warnings: Vec<NativeStatsError>,
    pub assets: Vec<NativeAsset>,
    #[napi(js_name = "outputPath")]
    pub output_path: String,
    #[napi(js_name = "watchDependencies")]
    pub watch_dependencies: NativeWatchDependencies,
}

#[napi(object)]
pub struct NativeModule {
    pub handle: u32,
    pub identifier: String,
    pub resource: String,
    #[napi(js_name = "type")]
    pub module_type: String,
    #[napi(js_name = "providedExports")]
    pub provided_exports: Option<Vec<String>>,
    #[napi(js_name = "usedExports")]
    pub used_exports: Option<Vec<String>>,
    #[napi(js_name = "allExportsUsed")]
    pub all_exports_used: bool,
}

#[napi(object)]
pub struct NativeModuleGraphConnection {
    pub handle: u32,
    #[napi(js_name = "originModuleHandle")]
    pub origin_module_handle: Option<u32>,
    #[napi(js_name = "moduleHandle")]
    pub module_handle: u32,
    #[napi(js_name = "resolvedModuleHandle")]
    pub resolved_module_handle: u32,
    #[napi(js_name = "dependencyType")]
    pub dependency_type: String,
    pub request: Option<String>,
    pub weak: bool,
    #[napi(js_name = "parentBlockIndex")]
    pub parent_block_index: i32,
}

#[napi(object)]
pub struct NativeChunk {
    pub handle: u32,
    pub name: Option<String>,
    #[napi(js_name = "renderId")]
    pub render_id: Option<Either<String, u32>>,
}

#[napi(object)]
pub struct NativeChunkGroup {
    pub handle: u32,
    pub name: Option<String>,
    #[napi(js_name = "chunkHandles")]
    pub chunk_handles: Vec<u32>,
    #[napi(js_name = "runtimeChunkHandle")]
    pub runtime_chunk_handle: Option<u32>,
    pub files: Vec<String>,
    #[napi(js_name = "isEntrypoint")]
    pub is_entrypoint: bool,
}

#[napi(object)]
pub struct NativeWatchDependencies {
    pub files: Vec<String>,
    pub contexts: Vec<String>,
    pub missing: Vec<String>,
}

#[napi(object)]
pub struct NativeInfrastructureError {
    pub name: String,
    pub message: String,
}

#[napi(object)]
pub struct NativeInfrastructureLogEvent {
    pub level: String,
    pub name: String,
    pub message: String,
}

#[napi(object, object_from_js = false)]
pub struct NativeRunResult {
    pub error: Option<NativeInfrastructureError>,
    pub stats: Option<NativeStatsJson>,
    pub compilation: Option<NativeCompilation>,
    pub logs: Vec<NativeInfrastructureLogEvent>,
}

#[napi]
pub struct NativeCompilation {
    module_graph: NativeModuleGraph,
    chunk_graph: unpack_core::ChunkGraph,
    assets: Vec<Asset>,
}

enum NativeModuleGraph {
    Owned(unpack_core::ModuleGraph),
    Leased {
        module_graph: unpack_core::ModuleGraph,
        return_sender: tokio::sync::oneshot::Sender<unpack_core::ModuleGraph>,
    },
    Released,
}

impl NativeCompilation {
    fn module_graph(&self) -> Result<&unpack_core::ModuleGraph> {
        match &self.module_graph {
            NativeModuleGraph::Owned(module_graph)
            | NativeModuleGraph::Leased { module_graph, .. } => Ok(module_graph),
            NativeModuleGraph::Released => Err(napi::Error::from_reason(
                "native compilation graph lease has been released",
            )),
        }
    }

    fn return_module_graph_lease(&mut self) -> Result<()> {
        let state = std::mem::replace(&mut self.module_graph, NativeModuleGraph::Released);
        let NativeModuleGraph::Leased {
            module_graph,
            return_sender,
        } = state
        else {
            self.module_graph = state;
            return Err(napi::Error::from_reason(
                "native compilation does not hold a module graph lease",
            ));
        };
        return_sender.send(module_graph).map_err(|module_graph| {
            self.module_graph = NativeModuleGraph::Owned(module_graph);
            napi::Error::from_reason("native compilation graph lease receiver was dropped")
        })
    }
}

impl Drop for NativeCompilation {
    fn drop(&mut self) {
        let state = std::mem::replace(&mut self.module_graph, NativeModuleGraph::Released);
        if let NativeModuleGraph::Leased {
            module_graph,
            return_sender,
        } = state
        {
            let _ = return_sender.send(module_graph);
        }
    }
}

#[napi]
impl NativeCompilation {
    #[napi]
    pub fn modules(&self) -> Result<Vec<NativeModule>> {
        Ok(self
            .module_graph()?
            .modules()
            .iter()
            .map(native_module)
            .collect())
    }

    #[napi(js_name = "takeAssetSources")]
    pub fn take_asset_sources(&mut self) -> Vec<NativeAssetSource> {
        native_asset_sources(std::mem::take(&mut self.assets))
    }

    #[napi(js_name = "clearAssetSources")]
    pub fn clear_asset_sources(&mut self) {
        self.assets.clear();
    }

    #[napi(js_name = "outgoingConnections")]
    pub fn outgoing_connections(
        &self,
        module_handle: u32,
    ) -> Result<Vec<NativeModuleGraphConnection>> {
        let module_graph = self.module_graph()?;
        let module_handle = unpack_core::ModuleHandle::new(module_handle as usize);
        if module_graph.module(module_handle).is_none() {
            return Ok(Vec::new());
        }
        Ok(module_graph
            .outgoing_connections(module_handle)
            .map(native_module_graph_connection)
            .collect())
    }

    #[napi(js_name = "incomingConnections")]
    pub fn incoming_connections(
        &self,
        module_handle: u32,
    ) -> Result<Vec<NativeModuleGraphConnection>> {
        let module_graph = self.module_graph()?;
        let module_handle = unpack_core::ModuleHandle::new(module_handle as usize);
        if module_graph.module(module_handle).is_none() {
            return Ok(Vec::new());
        }
        Ok(module_graph
            .incoming_connections(module_handle)
            .map(native_module_graph_connection)
            .collect())
    }

    #[napi(js_name = "connectionsByHandle")]
    pub fn connections_by_handle(
        &self,
        connection_handles: Vec<u32>,
    ) -> Result<Vec<NativeModuleGraphConnection>> {
        let module_graph = self.module_graph()?;
        Ok(connection_handles
            .into_iter()
            .filter_map(|handle| module_graph.connections().get(handle as usize))
            .map(native_module_graph_connection)
            .collect())
    }

    #[napi]
    pub fn chunks(&self) -> Result<Vec<NativeChunk>> {
        self.module_graph()?;
        Ok(self
            .chunk_graph
            .chunks()
            .iter()
            .map(|chunk| NativeChunk {
                handle: chunk.handle().index().try_into().unwrap_or(u32::MAX),
                name: chunk.name().map(str::to_string),
                render_id: native_render_id(chunk.render_id_string(), chunk.render_id_number()),
            })
            .collect())
    }

    #[napi(js_name = "chunkGroups")]
    pub fn chunk_groups(&self) -> Result<Vec<NativeChunkGroup>> {
        self.module_graph()?;
        Ok(self
            .chunk_graph
            .chunk_groups()
            .iter()
            .map(|group| {
                let chunks = group.chunks();
                NativeChunkGroup {
                    handle: group.handle().index().try_into().unwrap_or(u32::MAX),
                    name: match group.kind() {
                        unpack_core::ChunkGroupKind::Entrypoint { name } => Some(name.clone()),
                        unpack_core::ChunkGroupKind::Async => None,
                    },
                    chunk_handles: chunks
                        .iter()
                        .map(|chunk| chunk.index().try_into().unwrap_or(u32::MAX))
                        .collect(),
                    runtime_chunk_handle: chunks
                        .first()
                        .map(|chunk| chunk.index().try_into().unwrap_or(u32::MAX)),
                    files: chunks
                        .iter()
                        .filter_map(|handle| self.chunk_graph.chunk(*handle))
                        .map(|chunk| chunk.filename())
                        .collect(),
                    is_entrypoint: matches!(
                        group.kind(),
                        unpack_core::ChunkGroupKind::Entrypoint { .. }
                    ),
                }
            })
            .collect())
    }

    #[napi(js_name = "chunkEntryModules")]
    pub fn chunk_entry_modules(&self, chunk_handle: u32) -> Result<Vec<u32>> {
        self.module_graph()?;
        Ok(self
            .chunk_graph
            .chunk(unpack_core::ChunkHandle::new(chunk_handle as usize))
            .map(|chunk| {
                chunk
                    .root_modules()
                    .iter()
                    .copied()
                    .map(native_module_handle)
                    .collect()
            })
            .unwrap_or_default())
    }

    #[napi(js_name = "chunkModules")]
    pub fn chunk_modules(&self, chunk_handle: u32) -> Result<Vec<u32>> {
        self.module_graph()?;
        let chunk_handle = unpack_core::ChunkHandle::new(chunk_handle as usize);
        if self.chunk_graph.chunk(chunk_handle).is_none() {
            return Ok(Vec::new());
        }
        Ok(self
            .chunk_graph
            .chunk_modules(chunk_handle)
            .iter()
            .copied()
            .map(native_module_handle)
            .collect())
    }

    #[napi(js_name = "moduleChunks")]
    pub fn module_chunks(&self, module_handle: u32) -> Result<Vec<u32>> {
        let module_graph = self.module_graph()?;
        let module_handle = unpack_core::ModuleHandle::new(module_handle as usize);
        if module_graph.module(module_handle).is_none() {
            return Ok(Vec::new());
        }
        Ok(self
            .chunk_graph
            .module_chunks(module_handle)
            .iter()
            .map(|chunk| chunk.index().try_into().unwrap_or(u32::MAX))
            .collect())
    }

    #[napi(js_name = "moduleId")]
    pub fn module_id(&self, module_handle: u32) -> Result<Option<Either<String, u32>>> {
        let module_graph = self.module_graph()?;
        let module_handle = unpack_core::ModuleHandle::new(module_handle as usize);
        if module_graph.module(module_handle).is_none() {
            return Ok(None);
        }
        Ok(native_render_id(
            self.chunk_graph.module_render_id_string(module_handle),
            self.chunk_graph.module_render_id_number(module_handle),
        ))
    }

    #[napi(js_name = "returnModuleGraphLease")]
    pub fn return_module_graph_lease_to_compiler(&mut self) -> Result<()> {
        self.return_module_graph_lease()
    }
}

#[napi]
pub struct NativeAssets {
    state: NativeAssetsState,
    module_graph: unpack_core::ModuleGraph,
    chunk_graph: unpack_core::ChunkGraph,
}

enum NativeAssetsState {
    Leased {
        assets: Vec<Asset>,
        return_sender: tokio::sync::oneshot::Sender<Vec<Asset>>,
    },
    Released,
}

impl NativeAssets {
    fn assets_mut(&mut self) -> Result<&mut Vec<Asset>> {
        match &mut self.state {
            NativeAssetsState::Leased { assets, .. } => Ok(assets),
            NativeAssetsState::Released => Err(napi::Error::from_reason(
                "native assets lease has been released",
            )),
        }
    }

    fn return_lease(&mut self) -> Result<()> {
        let state = std::mem::replace(&mut self.state, NativeAssetsState::Released);
        let NativeAssetsState::Leased {
            assets,
            return_sender,
        } = state
        else {
            return Err(napi::Error::from_reason(
                "native assets lease has already been released",
            ));
        };
        return_sender
            .send(assets)
            .map_err(|_| napi::Error::from_reason("native assets lease receiver was dropped"))
    }
}

impl Drop for NativeAssets {
    fn drop(&mut self) {
        let state = std::mem::replace(&mut self.state, NativeAssetsState::Released);
        if let NativeAssetsState::Leased {
            assets,
            return_sender,
        } = state
        {
            let _ = return_sender.send(assets);
        }
    }
}

#[napi]
impl NativeAssets {
    #[napi]
    pub fn compilation(&mut self) -> NativeCompilation {
        NativeCompilation {
            module_graph: NativeModuleGraph::Owned(self.module_graph.clone()),
            chunk_graph: self.chunk_graph.clone(),
            assets: Vec::new(),
        }
    }

    #[napi(js_name = "takeAssetSources")]
    pub fn take_asset_sources(&mut self) -> Result<Vec<NativeAssetSource>> {
        Ok(native_asset_sources(std::mem::take(self.assets_mut()?)))
    }

    #[napi(js_name = "replaceAssetSources")]
    pub fn replace_asset_sources(&mut self, assets: Vec<NativeAssetSource>) -> Result<()> {
        *self.assets_mut()? = assets.into_iter().map(asset_from_native).collect();
        Ok(())
    }

    #[napi(js_name = "returnAssetsLease")]
    pub fn return_assets_lease(&mut self) -> Result<()> {
        self.return_lease()
    }
}

#[napi(object)]
pub struct NativeFlushResult {
    pub error: Option<NativeInfrastructureError>,
    pub logs: Vec<NativeInfrastructureLogEvent>,
}

#[napi(js_name = "createCompiler")]
pub fn create_compiler(
    options: NativeCompilerOptions,
    loader_callback: Option<
        Function<'_, FnArgs<(String, String, String, String)>, Promise<String>>,
    >,
    compilation_callback: Option<Function<'_, NativeCompilation, Promise<()>>>,
    finish_modules_callback: Option<Function<'_, NativeCompilation, Promise<()>>>,
    process_assets_callback: Option<Function<'_, NativeAssets, Promise<()>>>,
) -> Result<NativeCompiler> {
    init_internal_tracing_from_env();
    NativeCompiler::new(
        options,
        loader_callback,
        compilation_callback,
        finish_modules_callback,
        process_assets_callback,
    )
}

type NativeLoaderCallback = ThreadsafeFunction<
    FnArgs<(String, String, String, String)>,
    Promise<String>,
    FnArgs<(String, String, String, String)>,
    Status,
    false,
>;

struct NativeLoaderRunner {
    callback: Arc<NativeLoaderCallback>,
}

impl std::fmt::Debug for NativeLoaderRunner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("NativeLoaderRunner")
    }
}

impl LoaderRunner for NativeLoaderRunner {
    fn run(&self, request: LoaderRequest) -> LoaderFuture<'_> {
        let callback = Arc::clone(&self.callback);
        Box::pin(async move {
            let loader = request.loader.to_string_lossy().into_owned();
            let resource = request.resource.to_string_lossy().into_owned();
            let promise = callback
                .call_async_catch(FnArgs::from((
                    loader.clone(),
                    resource.clone(),
                    request.source,
                    request.options,
                )))
                .await
                .map_err(|error| CoreError::Loader {
                    loader: PathBuf::from(&loader),
                    path: PathBuf::from(&resource),
                    message: error.to_string(),
                })?;
            promise.await.map_err(|error| CoreError::Loader {
                loader: PathBuf::from(loader),
                path: PathBuf::from(resource),
                message: error.to_string(),
            })
        })
    }
}

type NativeFinishModulesCallback =
    ThreadsafeFunction<NativeCompilation, Promise<()>, NativeCompilation, Status, false, true>;
type NativeProcessAssetsCallback =
    ThreadsafeFunction<NativeAssets, Promise<()>, NativeAssets, Status, false, true>;

struct NativeCompilationHooks {
    compilation: Arc<NativeFinishModulesCallback>,
    finish_modules: Arc<NativeFinishModulesCallback>,
    process_assets: Arc<NativeProcessAssetsCallback>,
}

impl std::fmt::Debug for NativeCompilationHooks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("NativeCompilationHooks")
    }
}

impl CompilationHooks for NativeCompilationHooks {
    fn compilation<'a>(&'a self, compilation: &'a unpack_core::Compilation) -> HookFuture<'a> {
        call_compilation_hook(Arc::clone(&self.compilation), compilation)
    }

    fn finish_modules<'a>(
        &'a self,
        compilation: &'a mut unpack_core::Compilation,
    ) -> HookFuture<'a> {
        call_finish_modules_hook(Arc::clone(&self.finish_modules), compilation)
    }

    fn process_assets<'a>(
        &'a self,
        compilation: &'a mut unpack_core::Compilation,
    ) -> HookFuture<'a> {
        call_process_assets_hook(Arc::clone(&self.process_assets), compilation)
    }
}

fn call_compilation_hook<'a>(
    callback: Arc<NativeFinishModulesCallback>,
    _compilation: &'a unpack_core::Compilation,
) -> HookFuture<'a> {
    let native_compilation = NativeCompilation {
        module_graph: NativeModuleGraph::Owned(unpack_core::ModuleGraph::default()),
        chunk_graph: unpack_core::ChunkGraph::default(),
        assets: Vec::new(),
    };
    Box::pin(async move {
        let promise = callback
            .call_async_catch(native_compilation)
            .await
            .map_err(|error| CoreError::Hook {
                message: error.to_string(),
            })?;
        promise.await.map_err(|error| CoreError::Hook {
            message: error.to_string(),
        })
    })
}

fn call_finish_modules_hook<'a>(
    callback: Arc<NativeFinishModulesCallback>,
    compilation: &'a mut unpack_core::Compilation,
) -> HookFuture<'a> {
    let module_graph = compilation.take_module_graph();
    let (return_sender, return_receiver) = tokio::sync::oneshot::channel();
    let native_compilation = NativeCompilation {
        module_graph: NativeModuleGraph::Leased {
            module_graph,
            return_sender,
        },
        chunk_graph: unpack_core::ChunkGraph::default(),
        assets: Vec::new(),
    };
    Box::pin(async move {
        let callback_result = match callback.call_async_catch(native_compilation).await {
            Ok(promise) => promise.await.map_err(|error| CoreError::Hook {
                message: error.to_string(),
            }),
            Err(error) => Err(CoreError::Hook {
                message: error.to_string(),
            }),
        };

        let module_graph = return_receiver.await.map_err(|error| CoreError::Hook {
            message: format!("finishModules did not return the module graph lease: {error}"),
        })?;
        compilation.restore_module_graph(module_graph);
        callback_result
    })
}

fn call_process_assets_hook<'a>(
    callback: Arc<NativeProcessAssetsCallback>,
    compilation: &'a mut unpack_core::Compilation,
) -> HookFuture<'a> {
    let assets = compilation.take_assets();
    let (return_sender, return_receiver) = tokio::sync::oneshot::channel();
    let native_assets = NativeAssets {
        state: NativeAssetsState::Leased {
            assets,
            return_sender,
        },
        module_graph: compilation.module_graph().clone(),
        chunk_graph: compilation.chunk_graph().clone(),
    };
    Box::pin(async move {
        let callback_result = match callback.call_async_catch(native_assets).await {
            Ok(promise) => promise.await.map_err(|error| CoreError::Hook {
                message: error.to_string(),
            }),
            Err(error) => Err(CoreError::Hook {
                message: error.to_string(),
            }),
        };

        let assets = return_receiver.await.map_err(|error| CoreError::Hook {
            message: format!("processAssets did not return the assets lease: {error}"),
        })?;
        compilation.restore_assets(assets);
        callback_result
    })
}

#[napi]
pub struct NativeCompiler {
    compiler: Option<Arc<Compiler>>,
    output_path: PathBuf,
}

#[napi(object)]
pub struct NativeWatchChangeSet {
    #[napi(js_name = "modifiedFiles")]
    pub modified_files: Vec<String>,
    #[napi(js_name = "removedFiles")]
    pub removed_files: Vec<String>,
    #[napi(js_name = "changedContexts")]
    pub changed_contexts: Vec<String>,
}

#[napi(object)]
pub struct NativeRunOptions {
    #[napi(js_name = "idleReason")]
    pub idle_reason: Option<String>,
    #[napi(js_name = "isRebuild")]
    pub is_rebuild: Option<bool>,
    #[napi(js_name = "watchChangeSet")]
    pub watch_change_set: Option<NativeWatchChangeSet>,
}

#[napi]
impl NativeCompiler {
    #[napi]
    pub async fn run(&self, options: Option<NativeRunOptions>) -> NativeRunResult {
        let compiler = self.compiler.clone();
        let output_path = self.output_path.clone();
        let idle_reason = match options
            .as_ref()
            .and_then(|options| options.idle_reason.as_deref())
        {
            Some("largeChange") => CacheIdleReason::LargeChange,
            _ => CacheIdleReason::Ordinary,
        };
        let is_rebuild = options
            .as_ref()
            .and_then(|options| options.is_rebuild)
            .unwrap_or(false);
        let watch_change_set =
            options
                .and_then(|options| options.watch_change_set)
                .map(|changes| unpack_core::WatchChangeSet {
                    modified_files: changes
                        .modified_files
                        .into_iter()
                        .map(PathBuf::from)
                        .collect(),
                    removed_files: changes
                        .removed_files
                        .into_iter()
                        .map(PathBuf::from)
                        .collect(),
                    changed_contexts: changes
                        .changed_contexts
                        .into_iter()
                        .map(PathBuf::from)
                        .collect(),
                });

        run_compiler_inner(
            compiler,
            output_path,
            idle_reason,
            is_rebuild,
            watch_change_set,
        )
        .await
    }

    #[napi(js_name = "settleCache")]
    pub async fn settle_cache(&self) -> NativeFlushResult {
        let Some(compiler) = self.compiler.as_deref() else {
            return closed_cache_lifecycle_result();
        };
        cache_lifecycle_result(compiler.settle_cache().await)
    }

    #[napi(js_name = "flushCache")]
    pub async fn flush_cache(&self) -> NativeFlushResult {
        self.settle_cache().await
    }

    #[napi]
    pub async fn shutdown(&self) -> NativeFlushResult {
        let Some(compiler) = self.compiler.as_deref() else {
            return closed_cache_lifecycle_result();
        };
        cache_lifecycle_result(compiler.shutdown().await)
    }

    #[napi]
    pub fn close(&mut self) {
        self.compiler = None;
    }
}

fn init_internal_tracing_from_env() {
    let Some(filter) = internal_tracing_filter() else {
        return;
    };
    let Ok(env_filter) = EnvFilter::try_new(&filter) else {
        eprintln!("ignoring invalid {INTERNAL_TRACING_ENV} tracing filter: {filter}");
        return;
    };
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_span_events(FmtSpan::CLOSE)
        .with_target(true)
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
}

fn internal_tracing_filter() -> Option<String> {
    let value = std::env::var(INTERNAL_TRACING_ENV).ok()?;
    let value = value.trim();
    match value {
        "" | "0" | "false" | "False" | "FALSE" | "off" | "Off" | "OFF" | "none" | "None"
        | "NONE" => None,
        "1" | "true" | "True" | "TRUE" | "on" | "On" | "ON" => {
            Some(DEFAULT_INTERNAL_TRACING_FILTER.to_string())
        }
        _ => Some(value.to_string()),
    }
}

impl NativeCompiler {
    fn new(
        options: NativeCompilerOptions,
        loader_callback: Option<
            Function<'_, FnArgs<(String, String, String, String)>, Promise<String>>,
        >,
        compilation_callback: Option<Function<'_, NativeCompilation, Promise<()>>>,
        finish_modules_callback: Option<Function<'_, NativeCompilation, Promise<()>>>,
        process_assets_callback: Option<Function<'_, NativeAssets, Promise<()>>>,
    ) -> Result<Self> {
        let context = PathBuf::from(&options.context);
        let output_path = PathBuf::from(&options.output_path);
        let entries = options
            .entries
            .into_iter()
            .map(|entry| Entry::new(entry.name, entry.request))
            .collect::<Vec<_>>();
        let mut compiler_options = CompilerOptions::new(context, entries);
        compiler_options.cache = cache_options_from_native(options.cache)?;
        compiler_options.resolve_cache = options.resolve_cache;
        compiler_options.snapshot = snapshot_options_from_native(options.snapshot)?;
        compiler_options.infrastructure_logging =
            infrastructure_logging_options_from_native(options.infrastructure_logging);
        compiler_options.sourcemap = options.sourcemap;
        compiler_options.provided_exports = options.provided_exports;
        compiler_options.used_exports = options.used_exports;
        compiler_options.serial_rebuild_make = options.serial_rebuild_make;
        compiler_options.unsafe_watch_cache_invalidation = options.unsafe_watch_cache_invalidation;
        compiler_options.side_effects = match options.side_effects.as_str() {
            "disabled" => unpack_core::SideEffectsOption::Disabled,
            "flag" => unpack_core::SideEffectsOption::Flag,
            "analyze" => unpack_core::SideEffectsOption::Analyze,
            value => {
                return Err(napi::Error::from_reason(format!(
                    "options.optimization.sideEffects: unknown normalized value {value:?}"
                )));
            }
        };
        compiler_options.module_rules = options
            .module_rules
            .into_iter()
            .map(|rule| {
                let module_type = rule
                    .module_type
                    .as_deref()
                    .map(module_type_from_native)
                    .transpose()?;
                let module_rule = match rule.loader {
                    Some(loader) => ModuleRule::new(&rule.test, loader, rule.options),
                    None => ModuleRule::without_loader(&rule.test, rule.options),
                };
                module_rule
                    .map(|module_rule| module_rule.with_module_type(module_type))
                    .map(|module_rule| module_rule.with_side_effects(rule.side_effects))
                    .map_err(|error| {
                        napi::Error::from_reason(format!("options.module.rules[0].test: {error}"))
                    })
            })
            .collect::<Result<Vec<_>>>()?;
        compiler_options.loader_runner = loader_callback
            .map(|callback| {
                let callback: NativeLoaderCallback = callback
                    .build_threadsafe_function()
                    .callee_handled::<false>()
                    .build()?;
                Ok::<Arc<dyn LoaderRunner>, napi::Error>(Arc::new(NativeLoaderRunner {
                    callback: Arc::new(callback),
                }))
            })
            .transpose()?;
        compiler_options.compilation_hooks = compilation_callback
            .zip(finish_modules_callback)
            .zip(process_assets_callback)
            .map(
                |((compilation_callback, finish_modules_callback), process_assets_callback)| {
                    let compilation: NativeFinishModulesCallback = compilation_callback
                        .build_threadsafe_function()
                        .callee_handled::<false>()
                        .weak::<true>()
                        .build()?;
                    let finish_modules: NativeFinishModulesCallback = finish_modules_callback
                        .build_threadsafe_function()
                        .callee_handled::<false>()
                        .weak::<true>()
                        .build()?;
                    let process_assets: NativeProcessAssetsCallback = process_assets_callback
                        .build_threadsafe_function()
                        .callee_handled::<false>()
                        .weak::<true>()
                        .build()?;
                    Ok::<Arc<dyn CompilationHooks>, napi::Error>(Arc::new(NativeCompilationHooks {
                        compilation: Arc::new(compilation),
                        finish_modules: Arc::new(finish_modules),
                        process_assets: Arc::new(process_assets),
                    }))
                },
            )
            .transpose()?;
        let compiler = Compiler::new(compiler_options);

        Ok(Self {
            compiler: Some(Arc::new(compiler)),
            output_path,
        })
    }
}

fn module_type_from_native(module_type: &str) -> Result<ModuleType> {
    match module_type {
        "javascript/auto" => Ok(ModuleType::JavaScriptAuto),
        "json" => Ok(ModuleType::Json),
        "asset" => Ok(ModuleType::Asset),
        "asset/resource" => Ok(ModuleType::AssetResource),
        "asset/inline" => Ok(ModuleType::AssetInline),
        "asset/source" => Ok(ModuleType::AssetSource),
        value => Err(napi::Error::from_reason(format!(
            "unknown normalized module type {value:?}"
        ))),
    }
}

fn cache_options_from_native(options: NativeCacheOptions) -> Result<CacheOptions> {
    let mut cache = match options.cache_type.as_str() {
        "disabled" => CacheOptions::disabled(),
        "filesystem" => CacheOptions::filesystem(),
        _ => CacheOptions::memory(),
    };
    cache.cache_directory = options.cache_directory.map(PathBuf::from);
    cache.cache_location = options.cache_location.map(PathBuf::from);
    cache.name = options.name;
    cache.version = options.version;
    cache.build_dependencies = options
        .build_dependencies
        .into_iter()
        .map(|dependency| BuildDependency {
            name: dependency.name,
            requests: dependency.requests,
        })
        .collect();
    cache.automatic_build_dependencies = options
        .automatic_build_dependencies
        .into_iter()
        .map(PathBuf::from)
        .collect();
    cache.max_memory_generations = options
        .max_memory_generations
        .map(|generations| generations as u64);
    cache.cache_unaffected = options.cache_unaffected.unwrap_or(false)
        || options.memory_cache_unaffected.unwrap_or(false);
    if let Some(max_age) = options.max_age {
        if max_age.is_nan() || max_age < 0.0 {
            return Err(napi::Error::from_reason(
                "options.cache.maxAge must be a non-negative number",
            ));
        }
        cache.max_age = if max_age.is_infinite() {
            std::time::Duration::MAX
        } else {
            std::time::Duration::try_from_secs_f64(max_age / 1_000.0)
                .unwrap_or(std::time::Duration::MAX)
        };
    }
    cache.compression = match options.compression.as_deref() {
        None => CacheCompression::None,
        Some("gzip") => CacheCompression::Gzip,
        Some("brotli") => CacheCompression::Brotli,
        Some(_) => {
            return Err(napi::Error::from_reason(
                "options.cache.compression must be false, 'gzip', or 'brotli'",
            ));
        }
    };
    cache.allow_collecting_memory = options.allow_collecting_memory.unwrap_or(false);
    cache.idle_timeout = options.idle_timeout;
    cache.idle_timeout_for_initial_store = options.idle_timeout_for_initial_store;
    cache.idle_timeout_after_large_changes = options.idle_timeout_after_large_changes;
    cache.profile = options.profile.unwrap_or(false);
    cache.readonly = options.readonly.unwrap_or(false);
    Ok(cache)
}

fn snapshot_options_from_native(options: NativeSnapshotOptions) -> Result<SnapshotOptions> {
    Ok(SnapshotOptions {
        module: snapshot_strategy_from_native(options.module),
        resolve: snapshot_strategy_from_native(options.resolve),
        build_dependencies: snapshot_strategy_from_native(options.build_dependencies),
        resolve_build_dependencies: snapshot_strategy_from_native(
            options.resolve_build_dependencies,
        ),
        managed_paths: options
            .managed_paths
            .into_iter()
            .map(snapshot_path_pattern_from_native)
            .collect::<Result<Vec<_>>>()?,
        immutable_paths: options
            .immutable_paths
            .into_iter()
            .map(snapshot_path_pattern_from_native)
            .collect::<Result<Vec<_>>>()?,
        unmanaged_paths: options
            .unmanaged_paths
            .into_iter()
            .map(snapshot_path_pattern_from_native)
            .collect::<Result<Vec<_>>>()?,
    })
}

fn snapshot_strategy_from_native(strategy: NativeSnapshotStrategy) -> SnapshotStrategy {
    SnapshotStrategy {
        timestamp: strategy.timestamp,
        hash: strategy.hash,
    }
}

fn snapshot_path_pattern_from_native(
    pattern: NativeSnapshotPathPattern,
) -> Result<SnapshotPathPattern> {
    match pattern.pattern_type.as_str() {
        "path" => {
            let path = PathBuf::from(pattern.path.unwrap_or_default());
            Ok(SnapshotPathPattern::Path(
                fs::canonicalize(&path).unwrap_or(path),
            ))
        }
        "regexp" => {
            let source = pattern.source.unwrap_or_default();
            let flags = pattern.flags.unwrap_or_default();
            regex::RegexBuilder::new(&source)
                .case_insensitive(flags == "i")
                .build()
                .map_err(|error| {
                    napi::Error::from_reason(format!(
                        "snapshot path RegExp '{}' is not supported by Rust regex: {}",
                        source, error
                    ))
                })?;
            Ok(SnapshotPathPattern::Regex { source, flags })
        }
        "nodeModules" => Ok(SnapshotPathPattern::NodeModules),
        _ => Ok(SnapshotPathPattern::NodeModules),
    }
}

fn infrastructure_logging_options_from_native(
    options: NativeInfrastructureLoggingOptions,
) -> InfrastructureLoggingOptions {
    InfrastructureLoggingOptions {
        level: match options.level.as_str() {
            "error" => Some(InfrastructureLogLevel::Error),
            "warn" => Some(InfrastructureLogLevel::Warn),
            "info" => Some(InfrastructureLogLevel::Info),
            "log" => Some(InfrastructureLogLevel::Log),
            "verbose" => Some(InfrastructureLogLevel::Verbose),
            _ => None,
        },
    }
}

fn closed_cache_lifecycle_result() -> NativeFlushResult {
    NativeFlushResult {
        error: Some(NativeInfrastructureError {
            name: "CompilerClosedError".to_string(),
            message: "compiler is closed".to_string(),
        }),
        logs: Vec::new(),
    }
}

fn cache_lifecycle_result(outcome: unpack_core::CacheLifecycleOutcome) -> NativeFlushResult {
    NativeFlushResult {
        error: outcome
            .diagnostic()
            .map(|message| NativeInfrastructureError {
                name: "CacheFlushError".to_string(),
                message: message.to_string(),
            }),
        logs: infrastructure_log_events(outcome.infrastructure_log_events()),
    }
}

async fn run_compiler_inner(
    compiler: Option<Arc<Compiler>>,
    output_path: PathBuf,
    idle_reason: CacheIdleReason,
    is_rebuild: bool,
    watch_change_set: Option<unpack_core::WatchChangeSet>,
) -> NativeRunResult {
    let Some(compiler) = compiler else {
        return infrastructure_error("CompilerClosedError", "compiler is closed");
    };

    let pending = match compiler
        .run_until_finalize(idle_reason, is_rebuild, watch_change_set)
        .await
    {
        Ok(pending) => pending,
        Err(error) => {
            return infrastructure_error("InfrastructureError", error.to_string());
        }
    };

    let logs = infrastructure_log_events(pending.compilation().infrastructure_log_events());
    if let Err(error) = emit_assets(&output_path, pending.compilation().assets()) {
        return infrastructure_error_with_logs("OutputWriteError", error, logs);
    }
    let compilation = pending.finish();
    let stats = NativeStatsJson {
        errors: compilation.errors().iter().map(stats_error).collect(),
        warnings: Vec::new(),
        assets: compilation.assets().iter().map(asset_stats).collect(),
        output_path: output_path.to_string_lossy().into_owned(),
        watch_dependencies: watch_dependencies(compilation.watch_dependencies()),
    };
    let (module_graph, chunk_graph, assets) = compilation.into_parts();

    NativeRunResult {
        error: None,
        stats: Some(stats),
        compilation: Some(NativeCompilation {
            module_graph: NativeModuleGraph::Owned(module_graph),
            chunk_graph,
            assets,
        }),
        logs,
    }
}

fn native_render_id(
    string_id: Option<&str>,
    number_id: Option<u32>,
) -> Option<Either<String, u32>> {
    string_id
        .map(|value| Either::A(value.to_string()))
        .or_else(|| number_id.map(Either::B))
}

fn native_module_graph_connection(
    connection: &unpack_core::ModuleGraphConnection,
) -> NativeModuleGraphConnection {
    NativeModuleGraphConnection {
        handle: connection.handle.index().try_into().unwrap_or(u32::MAX),
        origin_module_handle: connection.origin_module.map(native_module_handle),
        module_handle: native_module_handle(connection.module),
        resolved_module_handle: native_module_handle(connection.resolved_module),
        dependency_type: dependency_type(&connection.dependency).to_string(),
        request: connection.dependency.request().map(str::to_string),
        weak: dependency_is_weak(&connection.dependency),
        parent_block_index: connection
            .origin_dependency_index
            .and_then(|id| i32::try_from(id.index()).ok())
            .unwrap_or(-1),
    }
}

fn native_module(module: &Module) -> NativeModule {
    let identity = module.identity();
    let resource = format!(
        "{}{}{}",
        identity.resource.to_string_lossy(),
        identity.query.as_deref().unwrap_or_default(),
        identity.fragment.as_deref().unwrap_or_default()
    );
    let request = if identity.loaders.is_empty() {
        resource.clone()
    } else {
        format!("{}!{resource}", identity.loaders.join("!"))
    };
    let module_type = match identity.module_type {
        ModuleType::JavaScriptAuto => "javascript/auto",
        ModuleType::Json => "json",
        ModuleType::Asset => "asset",
        ModuleType::AssetResource => "asset/resource",
        ModuleType::AssetInline => "asset/inline",
        ModuleType::AssetSource => "asset/source",
    };

    NativeModule {
        handle: native_module_handle(module.handle()),
        identifier: format!("{module_type}|{request}"),
        resource,
        module_type: module_type.to_string(),
        provided_exports: module
            .exports_info()
            .provided_exports()
            .map(|exports| exports.map(str::to_string).collect()),
        used_exports: module
            .exports_info()
            .used_exports()
            .map(|exports| exports.map(str::to_string).collect()),
        all_exports_used: module.exports_info().are_all_exports_used(),
    }
}

fn native_module_handle(handle: unpack_core::ModuleHandle) -> u32 {
    handle.index().try_into().unwrap_or(u32::MAX)
}

fn dependency_type(dependency: &Dependency) -> &'static str {
    match dependency {
        Dependency::Entry(_) => "entry",
        Dependency::HarmonyImportSideEffect(_) => "harmony side effect evaluation",
        Dependency::HarmonyImportSpecifier(_) => "harmony import specifier",
        Dependency::HarmonyExportHeader(_) => "harmony export header",
        Dependency::HarmonyExportSpecifier(_) => "harmony export specifier",
        Dependency::HarmonyExportExpression(_) => "harmony export expression",
        Dependency::HarmonyExportImportedSpecifier(_) => "harmony export imported specifier",
        Dependency::Null(_) => "null",
        Dependency::Const(_) => "const",
        Dependency::Import(_) => "import()",
    }
}

fn dependency_is_weak(dependency: &Dependency) -> bool {
    match dependency {
        Dependency::Entry(dependency) => dependency.module.weak,
        Dependency::HarmonyImportSideEffect(dependency) => dependency.module.weak,
        Dependency::HarmonyImportSpecifier(dependency) => dependency.module.weak,
        Dependency::HarmonyExportImportedSpecifier(dependency) => dependency.module.weak,
        Dependency::Import(dependency) => dependency.module.weak,
        Dependency::HarmonyExportHeader(_)
        | Dependency::HarmonyExportSpecifier(_)
        | Dependency::HarmonyExportExpression(_)
        | Dependency::Null(_)
        | Dependency::Const(_) => false,
    }
}

fn emit_assets(output_path: &Path, assets: &[Asset]) -> std::result::Result<(), String> {
    let span = tracing::trace_span!("unpack_node::emit_assets");
    let _enter = span.enter();
    fs::create_dir_all(output_path).map_err(|error| {
        format!(
            "failed to create output path {}: {error}",
            output_path.display()
        )
    })?;

    for asset in assets {
        let path = output_path.join(&asset.filename);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create asset directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        fs::write(&path, asset.source_bytes())
            .map_err(|error| format!("failed to write asset {}: {error}", path.display()))?;
    }

    Ok(())
}

fn infrastructure_error(name: impl Into<String>, message: impl Into<String>) -> NativeRunResult {
    infrastructure_error_with_logs(name, message, Vec::new())
}

fn infrastructure_error_with_logs(
    name: impl Into<String>,
    message: impl Into<String>,
    logs: Vec<NativeInfrastructureLogEvent>,
) -> NativeRunResult {
    NativeRunResult {
        error: Some(NativeInfrastructureError {
            name: name.into(),
            message: message.into(),
        }),
        stats: None,
        compilation: None,
        logs,
    }
}

fn stats_error(error: &CoreError) -> NativeStatsError {
    match error {
        CoreError::Resolve {
            issuer,
            request,
            message,
        } => NativeStatsError {
            message: error.to_string(),
            path: None,
            request: Some(request.clone()),
            issuer: Some(issuer.to_string_lossy().into_owned()),
            stack: Some(message.clone()),
        },
        CoreError::Read { path, message }
        | CoreError::Parse { path, message }
        | CoreError::Loader { path, message, .. }
        | CoreError::LoaderRules { path, message } => NativeStatsError {
            message: error.to_string(),
            path: Some(path.to_string_lossy().into_owned()),
            request: None,
            issuer: None,
            stack: Some(message.clone()),
        },
        CoreError::UnsupportedDynamicImport { path, message }
        | CoreError::ParseTask { path, message } => NativeStatsError {
            message: error.to_string(),
            path: Some(path.to_string_lossy().into_owned()),
            request: None,
            issuer: None,
            stack: Some(message.clone()),
        },
        CoreError::CompilerBusy
        | CoreError::CompilerClosed
        | CoreError::MissingModule(_)
        | CoreError::MissingModuleDirectory(_) => NativeStatsError {
            message: error.to_string(),
            path: None,
            request: None,
            issuer: None,
            stack: None,
        },
        CoreError::MakeTask { message }
        | CoreError::Hook { message }
        | CoreError::ModuleTypeRegistry { message } => NativeStatsError {
            message: error.to_string(),
            path: None,
            request: None,
            issuer: None,
            stack: Some(message.clone()),
        },
        CoreError::CodeGeneration { path, message, .. } => NativeStatsError {
            message: error.to_string(),
            path: Some(path.to_string_lossy().into_owned()),
            request: None,
            issuer: None,
            stack: Some(message.clone()),
        },
    }
}

fn asset_stats(asset: &Asset) -> NativeAsset {
    NativeAsset {
        name: asset.filename.clone(),
        size: asset.source_bytes().len().try_into().unwrap_or(u32::MAX),
    }
}

fn native_asset_sources(assets: Vec<Asset>) -> Vec<NativeAssetSource> {
    assets
        .into_iter()
        .map(|asset| NativeAssetSource {
            name: asset.filename,
            source: Buffer::from(
                asset
                    .binary_source
                    .unwrap_or_else(|| asset.source.into_bytes()),
            ),
        })
        .collect()
}

fn asset_from_native(asset: NativeAssetSource) -> Asset {
    let bytes = asset.source.to_vec();
    let source = String::from_utf8_lossy(&bytes).into_owned();
    let binary_source = std::str::from_utf8(&bytes).is_err().then_some(bytes);
    Asset {
        filename: asset.name,
        source,
        binary_source,
    }
}

fn infrastructure_log_events(
    events: &[InfrastructureLogEvent],
) -> Vec<NativeInfrastructureLogEvent> {
    events
        .iter()
        .map(|event| NativeInfrastructureLogEvent {
            level: event.level.as_str().to_string(),
            name: event.name.clone(),
            message: event.message.clone(),
        })
        .collect()
}

fn watch_dependencies(dependencies: &unpack_core::WatchDependencies) -> NativeWatchDependencies {
    NativeWatchDependencies {
        files: dependencies
            .files()
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        contexts: dependencies
            .contexts()
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        missing: dependencies
            .missing()
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_public_unaffected_options_enable_the_core_module_computation_cache() {
        let mut memory = native_cache_options("memory");
        memory.cache_unaffected = Some(true);
        assert!(cache_options_from_native(memory).unwrap().cache_unaffected);

        let mut filesystem = native_cache_options("filesystem");
        filesystem.memory_cache_unaffected = Some(true);
        assert!(
            cache_options_from_native(filesystem)
                .unwrap()
                .cache_unaffected
        );
    }

    fn native_cache_options(cache_type: &str) -> NativeCacheOptions {
        NativeCacheOptions {
            cache_type: cache_type.to_string(),
            cache_directory: None,
            cache_location: None,
            name: None,
            version: None,
            build_dependencies: Vec::new(),
            automatic_build_dependencies: Vec::new(),
            max_age: None,
            compression: None,
            allow_collecting_memory: None,
            max_memory_generations: None,
            cache_unaffected: None,
            memory_cache_unaffected: None,
            idle_timeout: None,
            idle_timeout_for_initial_store: None,
            idle_timeout_after_large_changes: None,
            profile: None,
            readonly: None,
        }
    }
}
