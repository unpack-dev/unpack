use serde::{Deserialize, Serialize};

use crate::SourceRange;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConstDependency {
    pub expression: String,
    pub range: SourceRange,
}

impl ConstDependency {
    pub fn new(expression: impl Into<String>, range: SourceRange) -> Self {
        Self {
            expression: expression.into(),
            range,
        }
    }
}
