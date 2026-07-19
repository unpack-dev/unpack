// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/NormalModule.js

use std::{
    fmt::Debug,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

use regex::Regex;

use crate::Result;

#[derive(Debug, Clone)]
pub struct ModuleRule {
    test: Regex,
    loader: Option<PathBuf>,
    module_type: Option<crate::ModuleType>,
    options: String,
    side_effects: Option<bool>,
}

impl ModuleRule {
    pub fn new(
        test: &str,
        loader: impl Into<PathBuf>,
        options: impl Into<String>,
    ) -> std::result::Result<Self, regex::Error> {
        Ok(Self {
            test: Regex::new(test)?,
            loader: Some(loader.into()),
            module_type: None,
            options: options.into(),
            side_effects: None,
        })
    }

    pub fn without_loader(
        test: &str,
        options: impl Into<String>,
    ) -> std::result::Result<Self, regex::Error> {
        Ok(Self {
            test: Regex::new(test)?,
            loader: None,
            module_type: None,
            options: options.into(),
            side_effects: None,
        })
    }

    pub fn with_module_type(mut self, module_type: Option<crate::ModuleType>) -> Self {
        self.module_type = module_type;
        self
    }

    pub fn with_side_effects(mut self, side_effects: Option<bool>) -> Self {
        self.side_effects = side_effects;
        self
    }

    pub(crate) fn side_effects(&self) -> Option<bool> {
        self.side_effects
    }

    pub(crate) fn module_type(&self) -> Option<crate::ModuleType> {
        self.module_type
    }

    pub(crate) fn matches(&self, resource: &Path) -> bool {
        self.test.is_match(&resource.to_string_lossy())
    }

    pub(crate) fn matched_loader(&self) -> Option<MatchedLoader> {
        self.loader.as_ref().map(|loader| MatchedLoader {
            identifier: format!("{}?{}", loader.to_string_lossy(), self.options),
            loader: loader.clone(),
            options: self.options.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedLoader {
    pub identifier: String,
    pub loader: PathBuf,
    pub options: String,
}

#[derive(Debug, Clone)]
pub struct LoaderRequest {
    pub loader: PathBuf,
    pub resource: PathBuf,
    pub source: String,
    pub options: String,
    pub module_runner: Arc<dyn LoaderModuleRunner>,
}

#[derive(Debug, Clone)]
pub struct LoadedLoaderModule {
    pub source: String,
    pub resource: PathBuf,
    pub identifier: String,
    pub file_dependencies: Vec<PathBuf>,
    pub dependency_requests: Vec<String>,
}

pub type LoaderModuleFuture<'a> =
    Pin<Box<dyn Future<Output = Result<LoadedLoaderModule>> + Send + 'a>>;

pub trait LoaderModuleRunner: Debug + Send + Sync {
    fn load(
        &self,
        request: String,
        kind: LoaderRequestKind,
        context: Option<PathBuf>,
    ) -> LoaderModuleFuture<'_>;
}

#[derive(Debug, Clone)]
pub struct LoaderResult {
    pub source: String,
    pub requests: Vec<LoaderModuleRequest>,
    pub file_dependencies: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct LoaderModuleRequest {
    pub kind: LoaderRequestKind,
    pub request: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderRequestKind {
    Load,
    Import,
}

pub type LoaderFuture<'a> = Pin<Box<dyn Future<Output = Result<LoaderResult>> + Send + 'a>>;

pub trait LoaderRunner: Debug + Send + Sync {
    fn run(&self, request: LoaderRequest) -> LoaderFuture<'_>;
}
