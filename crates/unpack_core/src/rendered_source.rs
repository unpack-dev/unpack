use rspack_sources::{
    ConcatSource, MapOptions, ObjectPool, RawStringSource, Source, SourceExt, SourceMap,
};

const SOURCE_MAP_FOOTER_TEMPLATE: &str = "\n//# sourceMappingURL=asset-render.map\n";

// The map template includes the same unmapped footer shape used during emission so restoring the
// materialized source preserves the original source graph's closing mapping. The real filename and
// sourceMappingURL remain per-Compilation finalization inputs and are never stored here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderedSource {
    source: String,
    source_map: Option<String>,
}

impl RenderedSource {
    pub(crate) fn new<T: Source + SourceExt + 'static>(source: T) -> Self {
        let rendered = source.source().into_string_lossy().into_owned();
        let mut mapped = ConcatSource::default();
        mapped.add(source);
        mapped.add(RawStringSource::from(SOURCE_MAP_FOOTER_TEMPLATE));
        let source_map = mapped
            .map(&ObjectPool::default(), &MapOptions::default())
            .map(|mut source_map| {
                source_map.set_file(None);
                source_map.to_json()
            });
        Self {
            source: rendered,
            source_map,
        }
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn source_map(&self) -> Option<&str> {
        self.source_map.as_deref()
    }

    pub(crate) fn persistent_parts(&self) -> (String, Option<String>) {
        (self.source.clone(), self.source_map.clone())
    }

    pub(crate) fn from_persistent_parts(
        source: String,
        source_map: Option<String>,
    ) -> Option<Self> {
        if let Some(source_map) = source_map.as_ref() {
            SourceMap::from_json(source_map.clone()).ok()?;
        }
        Some(Self { source, source_map })
    }
}
