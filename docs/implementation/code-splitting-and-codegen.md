# Code Splitting and Code Generation Implementation Plan

This plan implements dynamic-import code splitting and webpack-shaped code generation in `unpack_core`. The first implementation targets Node-style output with CommonJS `require` chunk loading and does not yet implement split chunks, CommonJS module parsing, loader pipelines, browser JSONP loading, or broad webpack public API parity.

## Target Shape

The compilation pipeline is explicit and keeps webpack's post-Make sealing boundary:

1. `make`
2. `seal`
   1. `build_chunk_graph`
   2. assign module and chunk render IDs
   3. `code_generation`
   4. `create_assets`

`Compiler::run()` should return a `Compilation` with a module graph, chunk graph, code generation results, and in-memory assets. Disk output, CLI, config files, and target selection are out of scope for the first implementation.

Generated output should use webpack-shaped internal names such as `__webpack_modules__`, `__webpack_module_cache__`, `__webpack_require__`, `__webpack_exports__`, `__webpack_require__.d`, `__webpack_require__.r`, `__webpack_require__.e`, `__webpack_require__.f.require`, and `__webpack_require__.u`.

## Slice 1: Dependency Model

Replace the current `{ kind, request }` dependency record with a webpack-like dependency enum using Rust-native shapes.

Add these concepts:

- `Dependency`
- `DependenciesBlock`
- `AsyncDependenciesBlock`
- `ModuleDependency`
- `NullDependency`
- `ConstDependency`
- `EntryDependency`
- `HarmonyImportSideEffectDependency`
- `HarmonyImportSpecifierDependency`
- `HarmonyExportHeaderDependency`
- `HarmonyExportSpecifierDependency`
- `HarmonyExportExpressionDependency`
- `HarmonyExportImportedSpecifierDependency`
- `ImportDependency`

Use UTF-8 byte offsets with half-open `[start, end)` ranges. Dependencies that can be resolved to modules expose a webpack-like `resource_identifier()`. `ConstDependency` and other presentational dependencies do not produce module graph connections.

`Module` should store:

- `dependencies`
- `blocks`
- `presentational_dependencies`
- original source text or `OriginalSource` input metadata
- build metadata needed by code generation

## Slice 2: Parser Output

Update the parser adapter to produce dependency records and async blocks instead of only request strings.

Initial coverage:

- Static side-effect imports
- Named imports
- Default imports
- Namespace imports
- Named exports
- Default exports
- Named re-exports
- Simple star re-exports
- Static-string dynamic imports

Static imports and re-exports should emit `HarmonyImportSideEffectDependency` separately from specifier dependencies. Import and export syntax removal should use presentational dependencies such as `ConstDependency` and `HarmonyExportHeaderDependency`.

Dynamic imports should create an `AsyncDependenciesBlock` containing an `ImportDependency`.

## Slice 3: Lexical Scope Collector

Add a minimal lexical scope collector to support webpack-like live binding rewrite.

The collector should track:

- Imported bindings
- Module, function, and block scopes
- Function parameters
- `var`, `let`, `const`, `function`, and `class` declarations
- Shadowing
- Identifier usage ranges for imported bindings

Imported binding reads and writes are rewritten to module-object property access. Do not reject imported binding assignment at parse time; webpack compiles it and lets the getter-only export binding fail at runtime.

## Slice 4: NormalModuleFactory and Make

Introduce `NormalModuleFactory` and move dependency factorization out of make task internals.

Make should:

- Group module dependencies by `resource_identifier()`.
- Factorize through `NormalModuleFactory`.
- Deduplicate resolved modules by `ModuleIdentity`.
- Create one module graph connection per resolved module dependency.
- Traverse dependencies from both module dependencies and async block dependencies.
- Preserve async block boundaries for chunk graph construction.

`ConstDependency`, `HarmonyExportHeaderDependency`, and other presentational dependencies should not be factorized.

## Slice 5: Chunk Graph

Add a separate chunk graph with webpack-like chunks and chunk groups.

Core types:

- `Chunk`
- `ChunkGraph`
- `ChunkGroup`
- `Entrypoint`

Relationships:

- module to chunk is many-to-many
- chunk to chunk group is many-to-many
- chunk group parents and children are sets
- chunk group chunks are ordered
- async blocks map to chunk groups through the chunk graph

First code splitting rules:

- Each entry creates an `Entrypoint` and initial chunk.
- Static reachability from the entry fills the initial chunk.
- Each async dependencies block creates or reuses an async chunk group and chunk.
- Async chunk collection starts from the block's resolved `ImportDependency` target.
- Modules already present in the parent entrypoint's initial chunk are excluded.
- Shared module extraction across entries or async chunks is not implemented.

Add `Chunk::split(new_chunk)` as the common rewiring operation for future split chunks.

## Slice 6: Code Generation Core

Add code generation based on `rspack_sources`.

Code generation is keyed only by module identity inside one `Compilation`. Each
renderable module produces exactly one source-preserving result plus its direct
runtime requirements. Results are not persisted or reused by later
compilations. Chunk asset rendering consumes those results and owns the module
factory wrapper.

Core pieces:

- `DependencyTemplate`
- `TemplateContext`
- `InitFragment`
- `RuntimeRequirement`
- `ExportsInfo`
- module code generation result

Use `OriginalSource` for module source, `ReplaceSource` for dependency replacements, and `ConcatSource` for wrappers and assets. `ExportsInfo` can be minimal: record provided exports, treat all exports as used, and return original used names.

Dependency templates can mutate source and append init fragments. Export bindings should be emitted through init fragments using `__webpack_require__.d`.

## Slice 7: Runtime and Asset Creation

Generate in-memory assets from chunks.

Initial asset:

- module factory table
- module cache
- `__webpack_require__`
- `__webpack_require__.d`
- `__webpack_require__.o`
- `__webpack_require__.r`
- `__webpack_require__.e`
- `__webpack_require__.f.require`
- `__webpack_require__.u`
- node require chunk installation
- entry startup

Async chunk asset:

```js
"use strict";
exports.id = "src_feature_js";
exports.ids = ["src_feature_js"];
exports.modules = {
  "./src/feature.js": ((__unused_webpack_module, __webpack_exports__, __webpack_require__) => {
  })
};
```

Include `exports.runtime` only when needed.

Module render ids should use webpack-like readable request strings relative to context, preserving query and fragment. Async chunk render ids should be derived from the async target module for the first implementation, while filename resolution should go through `__webpack_require__.u(chunkId)`.

## Slice 8: Tests

Use structure and runtime-semantics tests, not byte-for-byte webpack snapshots.

Fixture coverage:

- Static side-effect import
- Named import/export
- Default export
- Namespace import
- Live binding update
- Imported binding assignment runtime failure
- Named re-export
- Simple star re-export
- Dynamic import async chunk
- Async chunk excluding parent initial modules

Assertions:

- module graph connections exist per resolved module dependency
- async block maps to chunk group
- chunk group parent/child and ordered chunks are correct
- generated assets contain webpack-shaped runtime names
- dynamic import returns a promise and loads chunk via require
- live bindings observe updated exported values
- sourcemap plumbing can produce a map with original module sources

## Out of Scope

- Split chunks algorithm
- Browser JSONP loading
- CommonJS dependency parsing
- Loader pipeline
- Plugin API parity
- Tree shaking and used exports
- Export presence warnings/errors
- Full namespace interop helpers
- Context modules
- Magic comments
- Persistent cache implementation
