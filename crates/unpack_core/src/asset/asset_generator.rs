// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/asset/AssetGenerator.js

use std::{collections::BTreeMap, path::Path};

use crate::{
    ChunkGraph, Module, ModuleGraph, ModuleType,
    cache_hash::stable_hash,
    code_generation::Asset,
    code_generation_record::{CodeGenerationRecord, CodeGenerationSource},
    parser::ParsedModuleData,
    runtime::{RuntimeRequirement, RuntimeRequirements},
};

pub(crate) fn generate(module: &Module) -> CodeGenerationRecord {
    let ParsedModuleData::Asset { module_type } = module.parsed_data() else {
        unreachable!("asset modules must contain Asset Parser data")
    };
    let module_type = *module_type;
    let value = match module_type {
        ModuleType::AssetResource => resource_url(module),
        ModuleType::AssetInline => data_uri(&module.identity().resource, module.source_bytes()),
        ModuleType::AssetSource => String::from_utf8_lossy(module.source_bytes()).into_owned(),
        _ => unreachable!("effective asset module type must select a generator"),
    };
    let value = serde_json::to_string(&value).expect("asset module exports must serialize");
    let source = format!(
        "var __WEBPACK_ASSET_MODULE__ = {value};\n\
         __webpack_require__.d(__webpack_exports__, {{ default: () => (__WEBPACK_ASSET_MODULE__) }});"
    );
    let mut runtime_requirements = RuntimeRequirements::default();
    runtime_requirements.insert(RuntimeRequirement::DefinePropertyGetters);
    CodeGenerationRecord::new(CodeGenerationSource::Raw { source })
        .with_runtime_requirements(runtime_requirements)
}

pub(crate) fn render_resource_assets(
    module_graph: &ModuleGraph,
    chunk_graph: &ChunkGraph,
) -> Vec<Asset> {
    let mut assets = BTreeMap::new();
    for module in module_graph.modules() {
        if chunk_graph.module_chunks(module.handle()).is_empty()
            || !matches!(
                module.parsed_data(),
                ParsedModuleData::Asset {
                    module_type: ModuleType::AssetResource
                }
            )
        {
            continue;
        }
        assets
            .entry(resource_filename(module))
            .or_insert_with(|| module.source_bytes().to_vec());
    }
    assets
        .into_iter()
        .map(|(filename, binary_source)| Asset {
            filename,
            source: String::from_utf8_lossy(&binary_source).into_owned(),
            binary_source: Some(binary_source),
        })
        .collect()
}

fn resource_filename(module: &Module) -> String {
    let source = module.source_bytes();
    let first = stable_hash(&source);
    let second = stable_hash(&(first, source.len()));
    let mut hash = format!("{first:016x}{second:016x}");
    hash.truncate(20);
    let extension = module
        .identity()
        .resource
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    format!("{hash}{extension}")
}

fn resource_url(module: &Module) -> String {
    format!(
        "{}{}{}",
        resource_filename(module),
        module.identity().query.as_deref().unwrap_or_default(),
        module.identity().fragment.as_deref().unwrap_or_default()
    )
}

fn data_uri(path: &Path, source: &[u8]) -> String {
    let mime_type = mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream");
    format!("data:{mime_type};base64,{}", encode_base64(source))
}

fn encode_base64(source: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(source.len().div_ceil(3) * 4);
    for chunk in source.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        encoded.push(ALPHABET[(first >> 2) as usize] as char);
        encoded.push(ALPHABET[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            ALPHABET[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            ALPHABET[(third & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    encoded
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{data_uri, encode_base64};

    #[test]
    fn base64_encodes_complete_and_partial_groups() {
        assert_eq!(encode_base64(&[0, 1, 2, 250, 255]), "AAEC+v8=");
        assert_eq!(encode_base64(b"a"), "YQ==");
        assert_eq!(encode_base64(b"ab"), "YWI=");
    }

    #[test]
    fn data_uri_uses_mime_database_and_binary_fallback() {
        assert_eq!(
            data_uri(Path::new("image.avif"), b"x"),
            "data:image/avif;base64,eA=="
        );
        assert_eq!(
            data_uri(Path::new("data.unknown"), b"x"),
            "data:application/octet-stream;base64,eA=="
        );
    }
}
