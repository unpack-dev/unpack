use crate::Chunk;

pub(crate) fn resolve_chunk_filename(chunk: &Chunk) -> String {
    if let Some(filename) = chunk.filename_override() {
        return filename.to_string();
    }
    match chunk.name() {
        Some(name) => format!("{name}.js"),
        None => format!("{}.js", chunk.expect_id()),
    }
}
