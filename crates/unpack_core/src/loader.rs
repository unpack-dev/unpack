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
    options: String,
}

impl ModuleRule {
    pub fn new(
        test: &str,
        loader: impl Into<PathBuf>,
        options: impl Into<String>,
    ) -> std::result::Result<Self, regex::Error> {
        Ok(Self {
            test: Regex::new(test)?,
            loader: loader.into(),
            options: options.into(),
        })
    }

    pub(crate) fn matches(&self, resource: &Path) -> bool {
        self.test.is_match(&resource.to_string_lossy())
    }

    pub(crate) fn matched_loader(&self) -> MatchedLoader {
        MatchedLoader {
            identifier: format!("{}?{}", self.loader.to_string_lossy(), self.options),
            loader: self.loader.clone(),
            options: self.options.clone(),
        }
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
}

pub type LoaderFuture<'a> = Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;

pub trait LoaderRunner: Debug + Send + Sync {
    fn run(&self, request: LoaderRequest) -> LoaderFuture<'_>;
}
