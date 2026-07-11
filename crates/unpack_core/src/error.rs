use std::path::PathBuf;

use crate::ModuleHandle;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("compiler is already running")]
    CompilerBusy,

    #[error("compiler is shutting down or closed")]
    CompilerClosed,

    #[error("failed to resolve '{request}' from {issuer}: {message}")]
    Resolve {
        issuer: PathBuf,
        request: String,
        message: String,
    },

    #[error("failed to read {path}: {message}")]
    Read { path: PathBuf, message: String },

    #[error("failed to parse {path}: {message}")]
    Parse { path: PathBuf, message: String },

    #[error("loader {loader} failed for {path}: {message}")]
    Loader {
        loader: PathBuf,
        path: PathBuf,
        message: String,
    },

    #[error("loader rules failed for {path}: {message}")]
    LoaderRules { path: PathBuf, message: String },

    #[error("unsupported dynamic import in {path}: {message}")]
    UnsupportedDynamicImport { path: PathBuf, message: String },

    #[error("parser task failed for {path}: {message}")]
    ParseTask { path: PathBuf, message: String },

    #[error("make task failed: {message}")]
    MakeTask { message: String },

    #[error("module graph is missing module {0:?}")]
    MissingModule(ModuleHandle),

    #[error("module {0:?} does not have a filesystem parent directory")]
    MissingModuleDirectory(ModuleHandle),

    #[error("failed to generate {path} ({module:?}): {message}")]
    CodeGeneration {
        module: ModuleHandle,
        path: PathBuf,
        message: String,
    },
}

impl Error {
    pub fn is_compilation_error(&self) -> bool {
        matches!(
            self,
            Self::Resolve { .. }
                | Self::Read { .. }
                | Self::Parse { .. }
                | Self::Loader { .. }
                | Self::LoaderRules { .. }
                | Self::UnsupportedDynamicImport { .. }
                | Self::CodeGeneration { .. }
        )
    }

    pub(crate) fn resolve(
        issuer: impl Into<PathBuf>,
        request: impl Into<String>,
        source: rspack_resolver::ResolveError,
    ) -> Self {
        Self::Resolve {
            issuer: issuer.into(),
            request: request.into(),
            message: source.to_string(),
        }
    }

    pub(crate) fn read(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Read {
            path: path.into(),
            message: source.to_string(),
        }
    }
}
