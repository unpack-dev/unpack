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
    bindgen_prelude::{Either, FnArgs, Function, Promise},
    threadsafe_function::ThreadsafeFunction,
};
use napi_derive::napi;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use unpack_core::{
    Asset, BuildDependency, CacheCompression, CacheIdleReason, CacheOptions, Compiler,
    CompilerOptions, Dependency, Entry, Error as CoreError, InfrastructureLogEvent,
    InfrastructureLogLevel, InfrastructureLoggingOptions, LoaderFuture, LoaderRequest,
    LoaderRunner, Module, ModuleRule, ModuleType, SnapshotOptions, SnapshotPathPattern,
    SnapshotStrategy,
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
    pub snapshot: NativeSnapshotOptions,
    #[napi(js_name = "infrastructureLogging")]
    pub infrastructure_logging: NativeInfrastructureLoggingOptions,
    pub sourcemap: bool,
    #[napi(js_name = "moduleRules")]
    pub module_rules: Vec<NativeModuleRule>,
}

#[napi(object)]
pub struct NativeModuleRule {
    pub test: String,
    pub loader: String,
    pub options: String,
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
    pub id: u32,
    pub identifier: String,
    pub resource: String,
    #[napi(js_name = "type")]
    pub module_type: String,
    #[napi(js_name = "providedExports")]
    pub provided_exports: Vec<String>,
}

#[napi(object)]
pub struct NativeModuleGraphConnection {
    pub id: u32,
    #[napi(js_name = "originModule")]
    pub origin_module: Option<u32>,
    pub module: u32,
    #[napi(js_name = "dependencyType")]
    pub dependency_type: String,
    pub request: Option<String>,
    pub weak: bool,
    #[napi(js_name = "parentBlockIndex")]
    pub parent_block_index: i32,
}

#[napi(object)]
pub struct NativeChunk {
    pub id: u32,
    pub name: Option<String>,
    #[napi(js_name = "renderId")]
    pub render_id: Option<Either<String, u32>>,
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
    module_graph: Arc<unpack_core::ModuleGraph>,
    chunk_graph: Arc<unpack_core::ChunkGraph>,
}

#[napi]
impl NativeCompilation {
    #[napi]
    pub fn modules(&self) -> Vec<NativeModule> {
        self.module_graph
            .modules()
            .iter()
            .map(native_module)
            .collect()
    }

    #[napi(js_name = "incomingConnections")]
    pub fn incoming_connections(&self, module_id: u32) -> Vec<NativeModuleGraphConnection> {
        let module_id = unpack_core::ModuleId::new(module_id as usize);
        if self.module_graph.module(module_id).is_none() {
            return Vec::new();
        }
        self.module_graph
            .incoming_connections(module_id)
            .map(native_module_graph_connection)
            .collect()
    }

    #[napi(js_name = "outgoingConnections")]
    pub fn outgoing_connections(&self, module_id: u32) -> Vec<NativeModuleGraphConnection> {
        let module_id = unpack_core::ModuleId::new(module_id as usize);
        if self.module_graph.module(module_id).is_none() {
            return Vec::new();
        }
        self.module_graph
            .outgoing_connections(module_id)
            .map(native_module_graph_connection)
            .collect()
    }

    #[napi]
    pub fn chunks(&self) -> Vec<NativeChunk> {
        self.chunk_graph
            .chunks()
            .iter()
            .map(|chunk| NativeChunk {
                id: chunk.id().index().try_into().unwrap_or(u32::MAX),
                name: chunk.name().map(str::to_string),
                render_id: native_render_id(chunk.render_id_string(), chunk.render_id_number()),
            })
            .collect()
    }

    #[napi(js_name = "chunkModules")]
    pub fn chunk_modules(&self, chunk_id: u32) -> Vec<u32> {
        let chunk_id = unpack_core::ChunkId::new(chunk_id as usize);
        if self.chunk_graph.chunk(chunk_id).is_none() {
            return Vec::new();
        }
        self.chunk_graph
            .chunk_modules(chunk_id)
            .iter()
            .copied()
            .map(native_module_id)
            .collect()
    }

    #[napi(js_name = "moduleChunks")]
    pub fn module_chunks(&self, module_id: u32) -> Vec<u32> {
        let module_id = unpack_core::ModuleId::new(module_id as usize);
        if self.module_graph.module(module_id).is_none() {
            return Vec::new();
        }
        self.chunk_graph
            .module_chunks(module_id)
            .iter()
            .map(|chunk| chunk.index().try_into().unwrap_or(u32::MAX))
            .collect()
    }

    #[napi(js_name = "moduleId")]
    pub fn module_id(&self, module_id: u32) -> Option<Either<String, u32>> {
        let module_id = unpack_core::ModuleId::new(module_id as usize);
        if self.module_graph.module(module_id).is_none() {
            return None;
        }
        native_render_id(
            self.chunk_graph.module_render_id_string(module_id),
            self.chunk_graph.module_render_id_number(module_id),
        )
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
) -> Result<NativeCompiler> {
    init_internal_tracing_from_env();
    NativeCompiler::new(options, loader_callback)
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

#[napi]
pub struct NativeCompiler {
    compiler: Option<Arc<Compiler>>,
    output_path: PathBuf,
}

#[napi]
impl NativeCompiler {
    #[napi]
    pub async fn run(&self, idle_reason: Option<String>) -> NativeRunResult {
        let compiler = self.compiler.clone();
        let output_path = self.output_path.clone();
        let idle_reason = match idle_reason.as_deref() {
            Some("largeChange") => CacheIdleReason::LargeChange,
            _ => CacheIdleReason::Ordinary,
        };

        run_compiler_inner(compiler, output_path, idle_reason).await
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
        compiler_options.snapshot = snapshot_options_from_native(options.snapshot)?;
        compiler_options.infrastructure_logging =
            infrastructure_logging_options_from_native(options.infrastructure_logging);
        compiler_options.sourcemap = options.sourcemap;
        compiler_options.module_rules = options
            .module_rules
            .into_iter()
            .map(|rule| {
                ModuleRule::new(&rule.test, rule.loader, rule.options).map_err(|error| {
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
        let compiler = Compiler::new(compiler_options);

        Ok(Self {
            compiler: Some(Arc::new(compiler)),
            output_path,
        })
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
) -> NativeRunResult {
    let Some(compiler) = compiler else {
        return infrastructure_error("CompilerClosedError", "compiler is closed");
    };

    let pending = match compiler.run_until_finalize(idle_reason).await {
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
    let (module_graph, chunk_graph) = compilation.into_graphs();

    NativeRunResult {
        error: None,
        stats: Some(stats),
        compilation: Some(NativeCompilation {
            module_graph: Arc::new(module_graph),
            chunk_graph: Arc::new(chunk_graph),
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
        id: connection.id.index().try_into().unwrap_or(u32::MAX),
        origin_module: connection.origin_module.map(native_module_id),
        module: native_module_id(connection.module),
        dependency_type: dependency_type(&connection.dependency).to_string(),
        request: connection.dependency.request().map(str::to_string),
        weak: dependency_is_weak(&connection.dependency),
        parent_block_index: connection
            .origin_dependency_id
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
    };

    NativeModule {
        id: native_module_id(module.id()),
        identifier: format!("{module_type}|{request}"),
        resource,
        module_type: module_type.to_string(),
        provided_exports: module
            .exports_info()
            .provided_exports()
            .map(str::to_string)
            .collect(),
    }
}

fn native_module_id(id: unpack_core::ModuleId) -> u32 {
    id.index().try_into().unwrap_or(u32::MAX)
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
        fs::write(&path, &asset.source)
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
        CoreError::MakeTask { message } => NativeStatsError {
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
        size: asset.source.len().try_into().unwrap_or(u32::MAX),
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
