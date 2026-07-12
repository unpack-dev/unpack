// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/json/JsonGenerator.js

use serde_json::Value;

use crate::{
    code_generation_record::{CodeGenerationRecord, CodeGenerationSource},
    runtime::{RuntimeRequirement, RuntimeRequirements},
};

pub(crate) fn generate(source: &str) -> CodeGenerationRecord {
    let value = serde_json::from_str::<Value>(source)
        .expect("JSON module source must be validated during the build phase");
    let serialized = serde_json::to_string(&value).expect("JSON values must serialize");
    let serialized_string =
        serde_json::to_string(&serialized).expect("serialized JSON must serialize as a string");
    let mut exports = vec!["default: () => (__WEBPACK_JSON_MODULE__)".to_string()];
    if let Value::Object(object) = &value {
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
         __webpack_require__.r(__webpack_exports__);\n\
         __webpack_require__.d(__webpack_exports__, {{ {} }});",
        exports.join(", ")
    );
    let mut runtime_requirements = RuntimeRequirements::default();
    runtime_requirements.insert(RuntimeRequirement::MakeNamespaceObject);
    runtime_requirements.insert(RuntimeRequirement::DefinePropertyGetters);

    CodeGenerationRecord::new(CodeGenerationSource::Raw { source })
        .with_runtime_requirements(runtime_requirements)
}
