// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/javascript/JavascriptParser.js

use crate::{
    Result,
    normal_module_factory::ModuleParserContext,
    parser::{ParsedModule, parse_module_dependencies_with_hooks},
};

pub(crate) fn parse(context: ModuleParserContext<'_>) -> Result<ParsedModule> {
    parse_module_dependencies_with_hooks(
        context.resource,
        context.source,
        context.javascript_parser_hooks,
    )
}
