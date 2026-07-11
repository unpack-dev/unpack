use std::{
    fmt::Debug,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
};

use regex::Regex;

use crate::Result;

#[derive(Debug, Clone)]
pub struct ModuleRule {
    test: Regex,
    loader: PathBuf,
}

impl ModuleRule {
    pub fn new(test: &str, loader: impl Into<PathBuf>) -> std::result::Result<Self, regex::Error> {
        Ok(Self {
            test: Regex::new(test)?,
            loader: loader.into(),
        })
    }

    pub(crate) fn matches(&self, resource: &Path) -> bool {
        self.test.is_match(&resource.to_string_lossy())
    }

    pub(crate) fn loader(&self) -> &Path {
        &self.loader
    }
}

#[derive(Debug, Clone)]
pub struct LoaderRequest {
    pub loader: PathBuf,
    pub resource: PathBuf,
    pub source: String,
}

pub type LoaderFuture<'a> = Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;

pub trait LoaderRunner: Debug + Send + Sync {
    fn run(&self, request: LoaderRequest) -> LoaderFuture<'_>;
}
