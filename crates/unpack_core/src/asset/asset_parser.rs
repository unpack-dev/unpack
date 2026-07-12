// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/asset/AssetParser.js

use crate::{
    ModuleType, Result,
    normal_module_factory::ModuleParserContext,
    parser::{ParsedModule, ParsedModuleData},
};

const DEFAULT_MAX_INLINE_SIZE: usize = 8096;

pub(crate) fn parse(context: ModuleParserContext<'_>) -> Result<ParsedModule> {
    let module_type = match context.module_type {
        ModuleType::Asset if context.source_bytes.len() <= DEFAULT_MAX_INLINE_SIZE => {
            ModuleType::AssetInline
        }
        ModuleType::Asset => ModuleType::AssetResource,
        module_type => module_type,
    };
    Ok(ParsedModule {
        data: ParsedModuleData::Asset { module_type },
        ..ParsedModule::default()
    })
}
