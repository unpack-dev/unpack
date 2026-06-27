#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Dependency {
    pub kind: DependencyKind,
    pub request: String,
}

impl Dependency {
    pub fn new(kind: DependencyKind, request: impl Into<String>) -> Self {
        Self {
            kind,
            request: request.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DependencyKind {
    Entry,
    StaticImport,
    StaticExport,
    DynamicImport,
}
