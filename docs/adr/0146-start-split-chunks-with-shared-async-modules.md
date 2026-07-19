# Start Split Chunks with shared Async Chunk modules

Unpack will introduce `optimization.splitChunks` with the webpack-shaped
default boundary of `chunks: "async"`. The first slice groups modules by the
set of Async Chunks containing them, extracts groups meeting `minChunks` into a
shared Chunk, and inserts that Chunk into every affected Chunk Group. Dynamic
import code must ensure every Chunk in its Chunk Group before requiring the
target Module.

The public option initially accepts `chunks: "async"`, a positive `minChunks`,
and a string `name`. `minChunks` defaults to `2`; an explicit value of `1`
retains webpack's valid singleton-extraction behavior. A constant string name
combines all qualifying candidates into that one Chunk, so it is loaded by the
union of their affected Chunk Groups, matching webpack's named-chunk reuse.
Other Split Chunks options are rejected rather than accepted as no-ops.
Initial/all-chunk extraction is deferred until entry startup can load
multiple initial Chunks. Cache groups, size thresholds, request limits, module
tests, priorities, reuseExistingChunk, and per-group filename options remain
future model-backed slices.

This decision uses the many-to-many Chunk Graph, webpack-like Chunk Groups, and
Chunk split preparation established by ADRs 0015, 0016, and 0017. It does not
change the block-first refactoring trigger in DEV-001: per-block chunk options
still require moving beyond target-Module Async Chunk plan reuse.
