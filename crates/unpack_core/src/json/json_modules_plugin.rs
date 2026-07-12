// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/json/JsonModulesPlugin.js

use crate::{
    ModuleType,
    compiler::CompilerHookSet,
    json::{json_generator, json_parser},
    normal_module_factory::{ModuleSourceKind, ModuleTypeRegistration},
};

pub(crate) struct JsonModulesPlugin;

impl JsonModulesPlugin {
    pub(crate) fn apply(&self, hooks: &mut CompilerHookSet) {
        hooks.compilation.tap("JsonModulesPlugin", |hooks| {
            hooks.normal_module_factory_hooks.register(
                ModuleType::Json,
                ModuleTypeRegistration {
                    parser: json_parser::parse,
                    generator: json_generator::generate,
                    source_kind: ModuleSourceKind::Text,
                    side_effect_free: true,
                },
            );
        });
    }
}
