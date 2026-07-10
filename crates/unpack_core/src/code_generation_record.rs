use rspack_sources::{
    ConcatSource, OriginalSource, RawStringSource, ReplaceSource, Replacement, ReplacementEnforce,
};

use crate::code_generation::RuntimeRequirements;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeGenerationResult {
    source: ConcatSource,
    runtime_requirements: RuntimeRequirements,
}

impl CodeGenerationResult {
    pub(crate) fn new(record_source: CodeGenerationSource) -> Self {
        Self {
            source: record_source.render(),
            runtime_requirements: RuntimeRequirements::default(),
        }
    }

    pub(crate) fn source(&self) -> &ConcatSource {
        &self.source
    }

    pub(crate) fn runtime_requirements(&self) -> &RuntimeRequirements {
        &self.runtime_requirements
    }

    pub(crate) fn with_runtime_requirements(
        mut self,
        runtime_requirements: RuntimeRequirements,
    ) -> Self {
        self.runtime_requirements = runtime_requirements;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum CodeGenerationSource {
    Raw {
        source: String,
    },
    OriginalWithReplacements {
        prefix: String,
        original_source: String,
        original_name: String,
        replacements: Vec<CodeGenerationReplacement>,
        suffix: String,
    },
}

impl CodeGenerationSource {
    fn render(&self) -> ConcatSource {
        let mut source = ConcatSource::default();
        match self {
            Self::Raw { source: raw } => {
                source.add(RawStringSource::from(raw.clone()));
            }
            Self::OriginalWithReplacements {
                prefix,
                original_source,
                original_name,
                replacements,
                suffix,
            } => {
                let mut replaced = ReplaceSource::new(OriginalSource::new(
                    original_source.clone(),
                    original_name.clone(),
                ));
                for replacement in replacements {
                    replaced.replace_with_enforce(
                        replacement.start,
                        replacement.end,
                        replacement.content.clone(),
                        replacement.name.clone(),
                        replacement.enforce,
                    );
                }
                source.add(RawStringSource::from(prefix.clone()));
                source.add(replaced);
                source.add(RawStringSource::from(suffix.clone()));
            }
        }
        source
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CodeGenerationReplacement {
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) content: String,
    pub(crate) name: Option<String>,
    pub(crate) enforce: ReplacementEnforce,
}

impl From<&Replacement> for CodeGenerationReplacement {
    fn from(replacement: &Replacement) -> Self {
        Self {
            start: replacement.start(),
            end: replacement.end(),
            content: replacement.content().to_string(),
            name: replacement.name().map(str::to_string),
            enforce: replacement.enforce(),
        }
    }
}
