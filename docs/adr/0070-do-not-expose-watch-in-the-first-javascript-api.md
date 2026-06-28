# Do not expose watch in the first JavaScript API

The first JavaScript `Compiler` API will expose `run` and `close`, but not `watch`. Watch mode would require explicit invalidation, watcher lifecycle, incremental compilation, and additional close semantics, so it remains outside the first JavaScript API surface.
