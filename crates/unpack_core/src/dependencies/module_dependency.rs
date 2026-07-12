use std::fmt;

use serde::{Deserialize, Serialize};

use crate::SourceRange;

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModuleDependency {
    pub request: String,
    pub user_request: String,
    pub source_order: Option<usize>,
    pub range: Option<SourceRange>,
    pub weak: bool,
}

impl ModuleDependency {
    pub fn new(request: impl Into<String>, source_order: Option<usize>) -> Self {
        let request = request.into();
        Self {
            user_request: request.clone(),
            request,
            source_order,
            range: None,
            weak: false,
        }
    }

    pub fn resource_identifier(&self) -> String {
        format!("context|module{}", self.request)
    }
}

impl fmt::Debug for ModuleDependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModuleDependency")
            .field("request", &self.request)
            .field("source_order", &self.source_order)
            .field("range", &self.range)
            .finish()
    }
}
