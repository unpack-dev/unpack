# Keep unaffected module computations separate from Record Cache

Unpack will model webpack's `cacheUnaffected` and
`memoryCacheUnaffected` options with a Compiler-owned Module Computation Cache,
separate from the record-oriented Build Cache layers used for Module Build,
Code Generation, Asset Render, and PackFile storage. Both caches may share the
Compiler as their composition root, configuration, and diagnostics, but Module
Computation entries must not flow through ordinary cache facade `store`, layer
promotion, generation retention, or persistent publication.

As in webpack, explicitly enabling either cache option requires
`experiments.cacheUnaffected: true`. With the experiment enabled, development
mode defaults the memory `cacheUnaffected` or filesystem
`memoryCacheUnaffected` option to true; other modes default it to false.

The first typed memos are provided-exports analysis and static-reachability
collection for Chunk Graph construction. After Make, the cache compares
stable Module identities, built-source hashes, and outgoing graph references.
Changed and new modules are marked affected, and affected state propagates
conservatively through all incoming connections because Unpack does not yet
model webpack's dependency affect-type classification. Unaffected modules may
restore their provided-exports memo across Compilations owned by the same
Compiler; affected modules recompute and replace their memo. Filesystem
`memoryCacheUnaffected` remains active even when ordinary
`maxMemoryGenerations` is zero, and Module Computation entries are never
written to PackFile.

This supersedes ADR 0130's exclusion of cache-unaffected behavior and rejects
the earlier design that represented unaffected computations as an unbounded
Code Generation `MemoryCacheLayer`.
