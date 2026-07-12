// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/TemplatedPathPlugin.js

use crate::Chunk;

pub(crate) fn resolve_chunk_filename(chunk: &Chunk) -> String {
    if let Some(filename) = chunk.filename_override() {
        return filename.to_string();
    }
    match chunk.name() {
        Some(name) => format!("{name}.js"),
        None => format!("{}.js", chunk.render_id()),
    }
}
