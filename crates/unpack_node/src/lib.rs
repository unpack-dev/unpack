use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use napi::{Env, Result, Task, bindgen_prelude::AsyncTask};
use napi_derive::napi;
use unpack_core::{Asset, Compiler, CompilerOptions, Entry, Error as CoreError};

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
        let compiler = Compiler::new(CompilerOptions::new(context, entries));

        Self {
            compiler: Some(Arc::new(compiler)),
            output_path,
        }
    }
}

pub struct RunCompilerTask {
    compiler: Option<Arc<Compiler>>,
    output_path: PathBuf,
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
