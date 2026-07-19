# Align Module Concatenation with webpack's optimization phase

Unpack will expose `optimization.concatenateModules` only through a
webpack-recognizable `ModuleConcatenationPlugin` and `ConcatenatedModule`
responsibility. The plugin runs from `optimizeChunkModules`, after Chunk Graph
construction and before ID Assignment, and selects Harmony Modules using their
Chunk membership and incoming Module Graph connections. Module Concatenation
policy must not move into `buildChunkGraph` or ordinary dependency templates.

The public option is boolean and follows webpack's mode defaults: enabled in
production and disabled in development and none mode. A Concatenation Inner
Module must be in every Chunk of the Concatenation Root, must not be an Entry
Module, and must have only active concatenatable Harmony references within the
configuration's Chunks. Modules referenced from different Chunks remain
separate factories.

## Dense-handle adaptation

Webpack replaces the root Normal Module with a newly allocated
`ConcatenatedModule` and moves Module Graph connections to it. Unpack freezes
its dense `ModuleHandle` arena before the Finished Modules Boundary. The
current Rust adaptation therefore records an immutable `ConcatenatedModule`
plan on the Chunk Graph under the Concatenation Root's existing handle,
disconnects Inner Modules from the affected Chunks, and dispatches the root's
Code Generation through that plan. Module Graph connections remain attached to
their original Modules so dependency templates and public graph inspection
continue to address stable handles.

This handle-preserving shape is narrower than webpack's graph replacement. A
future model that can add optimization-created Modules without invalidating
compilation-local handles should replace it when public plugins need to observe
webpack's synthetic `ConcatenatedModule` identity.

## Code generation boundary

Webpack's `ConcatenatedModule` renames top-level bindings and directly flattens
module scopes. Unpack's parser currently retains dependency and import-usage
ranges but not the complete resolved top-level symbol/reference metadata needed
for that rename pass. The current `ConcatenatedModule` therefore emits one
module-table factory with configuration-local namespace objects and guarded
initializers. Internal Harmony imports call those initializers instead of
`__webpack_require__`; export getters, cyclic initialization, static import
order, Runtime Requirements, and each Original Source remain preserved.

Direct scope flattening is a follow-up trigger once the parser retains the
required binding metadata. It must remain owned by `ConcatenatedModule`, and it
must keep source maps and cyclic live-binding behavior covered by public
webpack comparison tests.
