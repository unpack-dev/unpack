// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/json/JsonParser.js

use std::path::Path;

use crate::{Error, Result, parser::ParsedModule};

pub(crate) fn parse(path: &Path, source: &str) -> Result<ParsedModule> {
    serde_json::from_str::<serde_json::Value>(source)
        .map(|_| ParsedModule::default())
        .map_err(|error| Error::Parse {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
}
