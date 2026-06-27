use std::path::PathBuf;

use crate::{Compilation, ResolveOptions, Result, UnpackResolver};

pub const DEFAULT_EXTENSIONS: &[&str] = &[".ts", ".tsx", ".js", ".jsx"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub request: String,
}

impl Entry {
    pub fn new(name: impl Into<String>, request: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            request: request.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompilerOptions {
    pub context: PathBuf,
    pub entries: Vec<Entry>,
    pub resolve: ResolveOptions,
    pub parallelism: usize,
}

impl CompilerOptions {
    pub fn new(context: impl Into<PathBuf>, entries: Vec<Entry>) -> Self {
        Self {
            context: normalize_context(context.into()),
            entries,
            resolve: default_resolve_options(),
            parallelism: 100,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Compiler {
    options: CompilerOptions,
    resolver: UnpackResolver,
}

impl Compiler {
    pub fn new(options: CompilerOptions) -> Self {
        let resolver = UnpackResolver::new(options.resolve.clone());
        Self { options, resolver }
    }

    pub fn options(&self) -> &CompilerOptions {
        &self.options
    }

    pub fn create_compilation(&self) -> Compilation {
        Compilation::new(self.options.clone(), self.resolver.clone())
    }

    pub async fn run(&self) -> Result<Compilation> {
        let mut compilation = self.create_compilation();
        compilation.make().await?;
        Ok(compilation)
    }
}

fn default_resolve_options() -> ResolveOptions {
    let mut options = ResolveOptions::default();
    options.extensions = DEFAULT_EXTENSIONS
        .iter()
        .map(|extension| (*extension).to_string())
        .collect();
    options
}

fn normalize_context(context: PathBuf) -> PathBuf {
    if context.is_absolute() {
        context
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&context))
            .unwrap_or(context)
    }
}
