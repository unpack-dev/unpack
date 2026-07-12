// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/asset/AssetParser.js

use crate::{ModuleType, parser::ParsedModule};

const DEFAULT_MAX_INLINE_SIZE: usize = 8096;

pub(crate) fn parse() -> ParsedModule {
    ParsedModule::default()
}

pub(crate) fn effective_module_type(module_type: ModuleType, size: usize) -> ModuleType {
    match module_type {
        ModuleType::Asset if size <= DEFAULT_MAX_INLINE_SIZE => ModuleType::AssetInline,
        ModuleType::Asset => ModuleType::AssetResource,
        module_type => module_type,
    }
}
