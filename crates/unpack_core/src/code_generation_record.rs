use rspack_sources::{
    ConcatSource, OriginalSource, RawStringSource, ReplaceSource, Replacement, ReplacementEnforce,
};

use crate::runtime::RuntimeRequirements;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeGenerationRecord {
    source: CodeGenerationSource,
    runtime_requirements: RuntimeRequirements,
}

impl CodeGenerationRecord {
    pub(crate) fn new(source: CodeGenerationSource) -> Self {
        Self {
            source,
            runtime_requirements: RuntimeRequirements::default(),
        }
    }

    pub(crate) fn source(&self) -> &CodeGenerationSource {
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

    pub(crate) fn is_compatible_with(&self, original_source: &str) -> bool {
        self.source.is_compatible_with(original_source)
    }

    pub(crate) fn into_result(self, original_source: &str) -> Option<CodeGenerationResult> {
        if !self.is_compatible_with(original_source) {
            return None;
        }
        Some(CodeGenerationResult {
            source: self.source.into_render(original_source),
            runtime_requirements: self.runtime_requirements,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeGenerationResult {
    source: ConcatSource,
    runtime_requirements: RuntimeRequirements,
}

impl CodeGenerationResult {
    #[cfg(test)]
    pub(crate) fn new(record_source: CodeGenerationSource) -> Self {
        CodeGenerationRecord::new(record_source)
            .into_result("")
            .expect("test Code Generation source should be compatible")
    }

    pub(crate) fn source(&self) -> &ConcatSource {
        &self.source
    }

    pub(crate) fn runtime_requirements(&self) -> &RuntimeRequirements {
        &self.runtime_requirements
    }

    #[cfg(test)]
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
        original_source_len: u32,
        original_name: String,
        replacements: Vec<CodeGenerationReplacement>,
        suffix: String,
    },
}

impl CodeGenerationSource {
    fn is_compatible_with(&self, original_source: &str) -> bool {
        let Self::OriginalWithReplacements {
            original_source_len,
            replacements,
            ..
        } = self
        else {
            return true;
        };
        if usize::try_from(*original_source_len).ok() != Some(original_source.len()) {
            return false;
        }
        replacements.iter().all(|replacement| {
            let Some((start, end)) = usize::try_from(replacement.start)
                .ok()
                .zip(usize::try_from(replacement.end).ok())
            else {
                return false;
            };
            start <= end
                && end <= original_source.len()
                && original_source.is_char_boundary(start)
                && original_source.is_char_boundary(end)
        })
    }

    fn into_render(self, original_source: &str) -> ConcatSource {
        let mut source = ConcatSource::default();
        match self {
            Self::Raw { source: raw } => {
                source.add(RawStringSource::from(raw));
            }
            Self::OriginalWithReplacements {
                prefix,
                original_source_len,
                original_name,
                replacements,
                suffix,
            } => {
                debug_assert_eq!(
                    usize::try_from(original_source_len).ok(),
                    Some(original_source.len())
                );
                let mut replaced = ReplaceSource::new(OriginalSource::new(
                    original_source.to_string(),
                    original_name,
                ));
                for replacement in replacements {
                    replaced.replace_with_enforce(
                        replacement.start,
                        replacement.end,
                        replacement.content,
                        replacement.name,
                        replacement.enforce,
                    );
                }
                source.add(RawStringSource::from(prefix));
                source.add(replaced);
                source.add(RawStringSource::from(suffix));
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

#[cfg(test)]
mod tests {
    use rspack_sources::ReplacementEnforce;

    use super::{CodeGenerationRecord, CodeGenerationReplacement, CodeGenerationSource};

    fn replacement_record(start: u32, end: u32, source_len: u32) -> CodeGenerationRecord {
        CodeGenerationRecord::new(CodeGenerationSource::OriginalWithReplacements {
            prefix: String::new(),
            original_source_len: source_len,
            original_name: "fixture.js".to_string(),
            replacements: vec![CodeGenerationReplacement {
                start,
                end,
                content: "replacement".to_string(),
                name: None,
                enforce: ReplacementEnforce::Normal,
            }],
            suffix: String::new(),
        })
    }

    #[test]
    fn replacement_recipe_must_match_current_utf8_source_boundaries() {
        let source = "éx";
        assert!(replacement_record(2, 3, 3).is_compatible_with(source));
        assert!(replacement_record(1, 3, 3).into_result(source).is_none());
        assert!(replacement_record(0, 1, 3).into_result(source).is_none());
        assert!(replacement_record(2, 3, 2).into_result(source).is_none());
    }
}
