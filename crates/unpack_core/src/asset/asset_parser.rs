// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/asset/AssetParser.js

use crate::{
    ModuleType,
    parser::{ParsedModule, ParsedModuleData},
};

const DEFAULT_MAX_INLINE_SIZE: usize = 8096;

pub(crate) fn parse(module_type: ModuleType, size: usize) -> ParsedModule {
    let module_type = match module_type {
        ModuleType::Asset if size <= DEFAULT_MAX_INLINE_SIZE => ModuleType::AssetInline,
        ModuleType::Asset => ModuleType::AssetResource,
        module_type => module_type,
    };
    ParsedModule {
        data: ParsedModuleData::Asset { module_type },
        ..ParsedModule::default()
    }
}
