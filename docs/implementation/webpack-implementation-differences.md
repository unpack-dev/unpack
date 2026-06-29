# Webpack implementation differences

This note compares the current Unpack implementation with the local webpack checkout at `/Users/bytedance/github/webpack` commit `10f5fccb2`. The goal is to identify gaps between current Unpack behavior and webpack so staged scope decisions can be separated from alignment gaps.

## Webpack alignment boundary

Unpack aims to align with webpack's public API shape and internal implementation model where practical. Existing ADRs define the current alignment shape:

- `0001-align-with-webpack-where-practical.md`: use webpack's public API and implementation model as the default reference, with documented deviations.
- `0018-use-webpack-like-compilation-pipeline.md`: keep recognizable make, chunk graph, code generation, and asset creation phases.
- `0031-test-webpack-shaped-output-by-structure-and-semantics.md`: test structure and runtime semantics, not byte-for-byte webpack snapshots.

That means differences should be classified as staged scope decisions, deliberate documented deviations, or alignment gaps. A difference is a current defect when it breaks a semantic Unpack has chosen to provide or a webpack-aligned surface it claims to expose.

## Compiler and compilation lifecycle

Unpack currently runs a direct Rust pipeline:

1. `Compilation::make`
2. `Compilation::build_chunk_graph`
3. `Compilation::create_assets`

Webpack's lifecycle is much larger: `Compiler.run` guards concurrent runs, calls run hooks, reads records, compiles, emits assets, emits records, stores build dependencies, and returns `Stats`. `Compiler.compile` creates normal and context module factories, runs make hooks, finishes the compilation, seals it, and then runs after-compile hooks.

Necessity:

- Keeping Unpack's lifecycle small is a staged implementation choice while the public API grows toward webpack.
- Matching webpack's hook graph, records, cache idle state, and plugin lifecycle becomes necessary when the corresponding public plugin and lifecycle surfaces are supported.
- The phase names are still useful because they provide good implementation boundaries and future cache boundaries.

## Normal module factory

Unpack's `NormalModuleFactory` only resolves a dependency request and builds a `ModuleIdentity`. The identity already has fields for `module_type`, `resource`, `query`, `fragment`, `layer`, and `loaders`, but today only `JavaScriptAuto` modules with no loaders are created.

Webpack's `NormalModuleFactory` owns a large part of public configurability: hooks, scheme-specific resolution, rule matching, loader resolution, parser/generator selection, layers, import attributes, dependency categories, file/missing/context dependency tracking, and ignored modules.

Necessity:

- The minimal Unpack factory is a staged scope reduction.
- The current `ModuleIdentity` shape is still useful because loaders, layers, module types, and query/fragment behavior can be added without redefining graph identity.
- Webpack's factory hook surface should be introduced when plugin and loader API work starts, using webpack names and ordering as the reference.

## Make phase and errors

Unpack uses `FuturesUnordered` plus a semaphore to factorize, read, parse, and connect modules. It records errors in `Compilation::errors`, but the first make error currently returns `Err` and stops the run.

Webpack uses separate async queues for factorize, add, build, rebuild, and process-dependencies. It can keep a failed module in the graph, collect module errors on the compilation, and continue sealing/emitting where the compilation itself completed.

Necessity:

- Rust-native queues are fine; copying webpack's `AsyncQueue` API is not needed.
- The current stop-on-first module-processing error is a real parity gap against Unpack's own ADRs `0037`, `0042`, and `0052`.
- Keeping failed modules in the graph and generating throwing module factories is necessary if the JavaScript API is expected to mirror webpack-like completed-compilation error semantics.

## Dependency model and parser

Unpack uses a minimal webpack-like dependency set for ESM imports, exports, re-exports, and static-string dynamic imports. It stores normal dependencies, async dependency blocks, and presentational dependencies separately, and code generation applies dependency templates to `rspack_sources` replacement sources.

Webpack supports a much wider parser surface: CommonJS, AMD, `import.meta`, context imports, dynamic import modes, magic comments, import attributes, weak/eager imports, deferred/source import phases, referenced-export tracking, branch guards, and detailed export presence behavior.

Necessity:

- The minimal ESM dependency taxonomy is necessary and useful; it keeps Unpack internally aligned with webpack concepts while the public API grows in staged slices.
- Rejecting context-module dynamic imports is an intentional first-scope limitation.
- CommonJS, import attributes, magic comments, and import modes are future feature choices, not required for the current ESM-first bundler.

## Module graph connections

Unpack's module graph connection records `origin_module`, `origin_block`, `dependency`, and resolved `module`.

Webpack's `ModuleGraphConnection` also tracks weak references, conditional active state, explanations, runtime-sensitive target activity, and mutable resolved module/origin fields.

Necessity:

- The simpler connection is correct for current always-active ESM dependencies.
- Conditional and weak connection states become necessary when Unpack implements dead-branch pruning, `webpackMode: "weak"`, side-effect connection state, export usage pruning, or import guards.
- Adding those fields before the corresponding features would increase complexity without current payoff.

## Chunk graph

Unpack creates one entrypoint chunk group per entry, assigns statically reachable modules to the initial chunk, creates or reuses async chunk groups for dynamic import targets, excludes modules already present in the parent initial chunk, and stores many-to-many module/chunk membership.

Webpack's `buildChunkGraph` is much broader. It tracks runtime per chunk group, chunk loading and async chunk flags, named async chunk reuse, async entrypoints, available-module masks, skipped modules, conditional connections, nested blocks, pre/post order indices, child/parent updates, and block-to-chunk-group links.

Necessity:

- Unpack's chunk group and many-to-many membership model is necessary because it preserves the shape needed for later split chunks and runtime chunks.
- Excluding parent initial modules is necessary for basic async chunk correctness.
- Split chunks, cache groups, min-size/min-chunks/max-request rules, runtime chunks, and named chunk options are not necessary for the first implementation.
- Nested async blocks are a current required semantic: Unpack only scans async blocks found in initial modules when creating async groups, while webpack recursively processes nested dependency blocks. A dynamic import reachable from an async chunk must create its own async chunk group; ignoring it is a parity gap against Unpack's chosen dynamic import semantics, not a compatibility luxury.

## Runtime and asset generation

Unpack emits webpack-shaped Node/CommonJS output with a module table, module cache, `__webpack_require__`, export getters, namespace marking, `__webpack_require__.e`, `__webpack_require__.f.require`, `__webpack_require__.u`, and require-based async chunk installation. It also emits source maps for generated assets.

Webpack renders runtime behavior through runtime modules and runtime requirements. Its Node require chunk loading module computes output paths, conditions loading on chunk type, handles installed chunk state, supports optional on-chunk-load hooks, external install hooks, HMR, base URI, and generated filename templates.

Necessity:

- Starting with fixed Node require chunk loading is intentional and covered by ADR `0032`.
- Hard-coded runtime helpers are acceptable while only one target exists, but the current `RuntimeRequirements` calculation is mostly aspirational because helper inclusion is not yet driven by it.
- Browser JSONP, ESM chunk loading, HMR, public path, chunk filename templates, and external chunk installation are future target features, not current requirements.

## ESM code generation

Unpack preserves source and applies templates for const replacements, import side effects, import specifier reads, export headers, export specifiers, default exports, re-exports, and dynamic imports. It intentionally uses webpack-shaped names and getter-based export bindings.

Webpack templates include additional behavior for used exports, inlined exports, export presence diagnostics, CommonJS/default interop, module concatenation, deferred imports, dead branch imports, async modules, namespace object variants, and precise star re-export conflict handling.

Necessity:

- Getter-based export bindings and import-usage rewrites are necessary for webpack-like ESM live binding semantics.
- Always treating exports as used is a deliberate first implementation.
- Full export usage pruning, ambiguous star export conflict handling, namespace/default interop, and module concatenation should be deferred until optimization or CommonJS interop becomes a goal.

## Render IDs and filenames

Unpack uses readable module render IDs relative to context and derives async chunk render IDs from target module paths. Webpack assigns module and chunk IDs through configurable id plugins and computes filenames through output templates.

Necessity:

- Readable render IDs are good for early debugging and semantic tests.
- Deterministic production IDs, hashing, and filename templates are future output-stability features.
- Byte-for-byte webpack output is not a goal.

## Current priority classification

Current staged scope limits:

- No broad webpack configuration, loader, plugin, or compilation API parity yet.
- Minimal Rust-native compiler and normal module factory.
- ESM-first parser surface.
- Fixed Node require chunk loading target.
- Readable render IDs and semantic tests.
- Byte-offset source ranges in Rust.

Implementation gaps to resolve for current stated semantics:

- Completed-compilation error behavior: failed modules should remain in the graph and emitted code should throw only if executed.
- Nested dynamic imports: async blocks discovered inside async chunks must create further async chunk groups.
- Runtime requirements should either drive helper emission or be kept clearly internal until needed.

Feature work to defer until explicitly chosen:

- Context modules and non-static dynamic imports.
- CommonJS parsing and interop.
- Loader and plugin API parity.
- Magic comments, dynamic import modes, import attributes, deferred/source import phases.
- Split chunks, cache groups, runtime chunks, HMR, browser/ESM/webworker chunk loading.
- Export usage analysis, tree shaking, module concatenation, and deterministic id plugins.
