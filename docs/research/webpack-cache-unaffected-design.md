# webpack `cacheUnaffected` design assessment

## Conclusion

webpack's unaffected-module cache and its ordinary `Cache.store` pipeline are
complementary caches at different abstraction levels. They may share a
compiler-owned composition root, configuration, lifecycle coordination, and
diagnostics, but they should not share the same entry type, backend fan-out, or
persistence layer.

An earlier Unpack implementation modeled unaffected caching as an extra
`MemoryCacheLayer` containing `CodeGeneration` entries. That did not match
webpack's design and was replaced before the public options were treated as
implemented.

## Ordinary `Cache.store`

webpack's root `Cache` exposes hook-based `get` and `store` operations keyed by
a string identifier and an ETag. A store is broadcast to registered backends;
backends include the ordinary memory cache and filesystem cache. Cache stages
order memory, disk, and network lookup. Values are cache records intended to be
restored by a later compilation and, when serializable, possibly a later
process.

Sources:

- [`lib/Cache.js`](https://github.com/webpack/webpack/blob/main/lib/Cache.js)
- [`lib/CacheFacade.js`](https://github.com/webpack/webpack/blob/main/lib/CacheFacade.js)
- [`lib/cache/MemoryCachePlugin.js`](https://github.com/webpack/webpack/blob/main/lib/cache/MemoryCachePlugin.js)
- [`lib/cache/MemoryWithGcCachePlugin.js`](https://github.com/webpack/webpack/blob/main/lib/cache/MemoryWithGcCachePlugin.js)

## `moduleMemCaches`

Enabling either memory-cache `cacheUnaffected` or filesystem-cache
`memoryCacheUnaffected` creates the same separate `compiler.moduleMemCaches =
new Map()`. It does not register another `compiler.cache` hook or backend. Both
options require the `experiments.cacheUnaffected` gate in webpack.

Unpack follows that gate and webpack's development defaults: enabling the
experiment defaults the cache-type-specific unaffected option to true in
development, while production and none modes leave it disabled unless the
cache option is explicitly enabled.

The compiler map is keyed by live `Module` object identity. Each item keeps the
module's `buildInfo`, weak dependency-to-module references, and a `WeakTupleMap`
of computed values. At the start of a later compilation,
`Compilation._computeAffectedModules` compares build-info identity and outgoing
references, replaces caches for changed modules, and propagates affected state
through incoming graph connections. A second pass validates chunk-graph
references before reusing chunk-dependent computations.

Cached values are fine-grained computations such as dependency queries,
provided-export restoration, block-to-module maps, dependency diagnostics,
runtime requirements, and module hashes. They are not ordinary serialized
module-build or code-generation records.

Sources:

- [`lib/WebpackOptionsApply.js`](https://github.com/webpack/webpack/blob/main/lib/WebpackOptionsApply.js)
- [`lib/Compiler.js`](https://github.com/webpack/webpack/blob/main/lib/Compiler.js)
- [`lib/Compilation.js`](https://github.com/webpack/webpack/blob/main/lib/Compilation.js)
- [`lib/ModuleGraph.js`](https://github.com/webpack/webpack/blob/main/lib/ModuleGraph.js)
- [`lib/buildChunkGraph.js`](https://github.com/webpack/webpack/blob/main/lib/buildChunkGraph.js)

## The caches deliberately overlap

`FlagDependencyExportsPlugin` demonstrates the intended relationship. It first
tries a module-local mem-cache entry, then falls back to the ordinary
compilation cache. After computation it writes both: the cheap live-object
memoization entry and a separate identifier/ETag cache record. This is an
explicit two-cache design, not one entry flowing through two storage layers.

Source:

- [`lib/FlagDependencyExportsPlugin.js`](https://github.com/webpack/webpack/blob/main/lib/FlagDependencyExportsPlugin.js)

## Implication for Unpack

Unpack may keep both under `BuildCache` as a composition root, but it should
introduce a distinct compiler-owned module-computation cache. A Rust-native
shape could use stable module identities plus typed memo slots instead of
copying webpack's `WeakTupleMap`, while preserving these boundaries:

1. ordinary cache facades continue to own restorable module-build,
   code-generation, and asset-render records;
2. unaffected caching owns only in-process graph-derived computations;
3. invalidation compares module build identity and graph references, then
   propagates affected state;
4. chunk-graph-dependent memoization is validated after IDs and chunk
   relationships are known;
5. unaffected entries are never sent to PackFile or promoted by an ordinary
   cache-layer lookup;
6. typed consumers such as provided-exports analysis and static reachability
   query and store their own memo slots without moving their algorithms into
   the cache;
7. tests use repeated compilations on one compiler and prove that changing a
   dependency invalidates the affected closure while an unrelated subgraph
   retains memoized computations.

The repository's existing ADR also records cache-unaffected behavior as a
model-backed exclusion, so exposing the options requires implementing this
model or updating the ADR with a deliberate deviation:

- [`docs/adr/0130-align-public-cache-and-snapshot-contracts-with-webpack.md`](../adr/0130-align-public-cache-and-snapshot-contracts-with-webpack.md)
