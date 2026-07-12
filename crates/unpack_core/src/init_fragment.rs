// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/InitFragment.js

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InitFragment {
    stage: InitFragmentStage,
    order: usize,
    content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum InitFragmentStage {
    Compatibility,
    Export,
    Import,
    StarReexport,
}

impl InitFragment {
    pub(crate) fn new(stage: InitFragmentStage, order: usize, content: String) -> Self {
        Self {
            stage,
            order,
            content,
        }
    }

    pub(crate) fn render(mut fragments: Vec<Self>) -> String {
        fragments.sort_by_key(|fragment| (fragment.stage, fragment.order));
        fragments
            .into_iter()
            .map(|fragment| fragment.content)
            .collect()
    }
}
