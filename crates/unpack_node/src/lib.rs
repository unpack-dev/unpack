use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use napi::{Env, Result, Task, bindgen_prelude::AsyncTask};
use napi_derive::napi;
use unpack_core::{
    Asset, BuildDependency, CacheOptions, Compiler, CompilerOptions, Entry, Error as CoreError,
    SnapshotOptions, SnapshotStrategy,
};

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
    #[napi(js_name = "maxMemoryGenerations")]
    pub max_memory_generations: Option<u32>,
    #[napi(js_name = "idleTimeout")]
    pub idle_timeout: Option<u32>,
}

#[napi(object)]
pub struct NativeBuildDependency {
    pub name: String,
    pub files: Vec<String>,
}

#[napi(object)]
pub struct NativeSnapshotOptions {
    pub module: NativeSnapshotStrategy,
    #[napi(js_name = "buildDependencies")]
    pub build_dependencies: NativeSnapshotStrategy,
}

#[napi(object)]
pub struct NativeSnapshotStrategy {
    pub timestamp: bool,
    pub hash: bool,
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
}

#[napi(object)]
pub struct NativeInfrastructureError {
    pub name: String,
    pub message: String,
}

#[napi(object)]
pub struct NativeRunResult {
    pub error: Option<NativeInfrastructureError>,
    pub stats: Option<NativeStatsJson>,
}

#[napi(object)]
pub struct NativeFlushResult {
    pub error: Option<NativeInfrastructureError>,
}

#[napi(js_name = "createCompiler")]
pub fn create_compiler(options: NativeCompilerOptions) -> NativeCompiler {
    NativeCompiler::new(options)
}

#[napi]
pub struct NativeCompiler {
    compiler: Option<Arc<Compiler>>,
    output_path: PathBuf,
}

#[napi]
impl NativeCompiler {
    #[napi]
    pub fn run(&self) -> AsyncTask<RunCompilerTask> {
        AsyncTask::new(RunCompilerTask {
            compiler: self.compiler.clone(),
            output_path: self.output_path.clone(),
        })
    }

    #[napi(js_name = "flushCache")]
    pub fn flush_cache(&self) -> AsyncTask<FlushCacheTask> {
        AsyncTask::new(FlushCacheTask {
            compiler: self.compiler.clone(),
        })
    }

    #[napi]
    pub fn close(&mut self) {
        self.compiler = None;
    }
}

impl NativeCompiler {
    fn new(options: NativeCompilerOptions) -> Self {
        let context = PathBuf::from(&options.context);
        let output_path = PathBuf::from(&options.output_path);
        let entries = options
            .entries
            .into_iter()
            .map(|entry| Entry::new(entry.name, entry.request))
            .collect::<Vec<_>>();
        let mut compiler_options = CompilerOptions::new(context, entries);
        compiler_options.cache = cache_options_from_native(options.cache);
        compiler_options.snapshot = snapshot_options_from_native(options.snapshot);
        let compiler = Compiler::new(compiler_options);

        Self {
            compiler: Some(Arc::new(compiler)),
            output_path,
        }
    }
}

fn cache_options_from_native(options: NativeCacheOptions) -> CacheOptions {
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
            files: dependency.files.into_iter().map(PathBuf::from).collect(),
        })
        .collect();
    cache.max_memory_generations = options.max_memory_generations;
    cache.idle_timeout = options.idle_timeout;
    cache
}

fn snapshot_options_from_native(options: NativeSnapshotOptions) -> SnapshotOptions {
    SnapshotOptions {
        module: snapshot_strategy_from_native(options.module),
        build_dependencies: snapshot_strategy_from_native(options.build_dependencies),
    }
}

fn snapshot_strategy_from_native(strategy: NativeSnapshotStrategy) -> SnapshotStrategy {
    SnapshotStrategy {
        timestamp: strategy.timestamp,
        hash: strategy.hash,
    }
}

pub struct RunCompilerTask {
    compiler: Option<Arc<Compiler>>,
    output_path: PathBuf,
}

pub struct FlushCacheTask {
    compiler: Option<Arc<Compiler>>,
}

impl Task for RunCompilerTask {
    type Output = NativeRunResult;
    type JsValue = NativeRunResult;

    fn compute(&mut self) -> Result<Self::Output> {
        Ok(run_compiler_inner(
            self.compiler.as_deref(),
            &self.output_path,
        ))
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

impl Task for FlushCacheTask {
    type Output = NativeFlushResult;
    type JsValue = NativeFlushResult;

    fn compute(&mut self) -> Result<Self::Output> {
        let Some(compiler) = self.compiler.as_deref() else {
            return Ok(NativeFlushResult {
                error: Some(NativeInfrastructureError {
                    name: "CompilerClosedError".to_string(),
                    message: "compiler is closed".to_string(),
                }),
            });
        };

        Ok(match compiler.flush_cache() {
            Ok(()) => NativeFlushResult { error: None },
            Err(message) => NativeFlushResult {
                error: Some(NativeInfrastructureError {
                    name: "CacheFlushError".to_string(),
                    message,
                }),
            },
        })
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

fn run_compiler_inner(compiler: Option<&Compiler>, output_path: &Path) -> NativeRunResult {
    let Some(compiler) = compiler else {
        return infrastructure_error("CompilerClosedError", "compiler is closed");
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            return infrastructure_error("InfrastructureError", error.to_string());
        }
    };

    let compilation = match runtime.block_on(compiler.run()) {
        Ok(compilation) => compilation,
        Err(error) => {
            return infrastructure_error("InfrastructureError", error.to_string());
        }
    };

    if let Err(error) = emit_assets(output_path, compilation.assets()) {
        return infrastructure_error("OutputWriteError", error);
    }

    NativeRunResult {
        error: None,
        stats: Some(NativeStatsJson {
            errors: compilation.errors().iter().map(stats_error).collect(),
            warnings: Vec::new(),
            assets: compilation.assets().iter().map(asset_stats).collect(),
            output_path: output_path.to_string_lossy().into_owned(),
        }),
    }
}

fn emit_assets(output_path: &Path, assets: &[Asset]) -> std::result::Result<(), String> {
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
    NativeRunResult {
        error: Some(NativeInfrastructureError {
            name: name.into(),
            message: message.into(),
        }),
        stats: None,
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
        CoreError::Read { path, message } | CoreError::Parse { path, message } => {
            NativeStatsError {
                message: error.to_string(),
                path: Some(path.to_string_lossy().into_owned()),
                request: None,
                issuer: None,
                stack: Some(message.clone()),
            }
        }
        CoreError::UnsupportedDynamicImport { path, message }
        | CoreError::ParseTask { path, message } => NativeStatsError {
            message: error.to_string(),
            path: Some(path.to_string_lossy().into_owned()),
            request: None,
            issuer: None,
            stack: Some(message.clone()),
        },
        CoreError::MissingModule(_) | CoreError::MissingModuleDirectory(_) => NativeStatsError {
            message: error.to_string(),
            path: None,
            request: None,
            issuer: None,
            stack: None,
        },
    }
}

fn asset_stats(asset: &Asset) -> NativeAsset {
    NativeAsset {
        name: asset.filename.clone(),
        size: asset.source.len().try_into().unwrap_or(u32::MAX),
    }
}
