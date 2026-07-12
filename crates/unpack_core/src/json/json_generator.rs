// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/json/JsonGenerator.js

use crate::{
    Result,
    code_generation_record::{CodeGenerationRecord, CodeGenerationSource},
    normal_module_factory::ModuleGeneratorContext,
    parser::ParsedModuleData,
    runtime::{RuntimeRequirement, RuntimeRequirements},
};

pub(crate) fn generate(context: ModuleGeneratorContext<'_>) -> Result<CodeGenerationRecord> {
    let ParsedModuleData::Json(value) = context.module.parsed_data() else {
        unreachable!("JSON modules must contain JSON Parser data")
    };
    let serialized = serde_json::to_string(&value).expect("JSON values must serialize");
    let serialized_string =
        serde_json::to_string(&serialized).expect("serialized JSON must serialize as a string");
    let mut exports = vec!["default: () => (__WEBPACK_JSON_MODULE__)".to_string()];
    if let serde_json::Value::Object(object) = value {
        exports.extend(
            object
                .keys()
                .filter(|name| name.as_str() != "default")
                .map(|name| {
                    let name =
                        serde_json::to_string(name).expect("JSON object keys must serialize");
                    format!("[{name}]: () => (__WEBPACK_JSON_MODULE__[{name}])")
                }),
        );
    }

    let source = format!(
        "var __WEBPACK_JSON_MODULE__ = JSON.parse({serialized_string});\n\
         __webpack_require__.d(__webpack_exports__, {{ {} }});",
        exports.join(", ")
    );
    let mut runtime_requirements = RuntimeRequirements::default();
    runtime_requirements.insert(RuntimeRequirement::DefinePropertyGetters);

    Ok(
        CodeGenerationRecord::new(CodeGenerationSource::Raw { source })
            .with_runtime_requirements(runtime_requirements),
    )
}
