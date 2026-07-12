# Webpack implementation differences

This note compares current Unpack behavior with the repository-pinned
`webpack@5.108.1` and current source-layout decisions with webpack commit
`da91761ed92c8e133ee321c7db4ad6c4698cae0a`. The goal is to identify gaps
between current Unpack behavior and webpack so staged scope decisions can be
separated from alignment gaps. The central classification and refactoring index
is `webpack-architecture-deviation-register.md`.

## Webpack alignment boundary

Unpack explores how far bundler performance can be pushed while aligning as closely as possible with webpack's architecture and functionality. Existing ADRs define the current alignment constraints:

- `0137-explore-performance-ceiling-while-aligning-with-webpack.md`: use webpack's architecture and functionality as the reference constraints for performance exploration, with documented deviations.
- `0018-use-webpack-like-compilation-pipeline.md`: keep recognizable make, chunk graph, code generation, and asset creation phases.
- `0031-test-webpack-shaped-output-by-structure-and-semantics.md`: test structure and runtime semantics, not byte-for-byte webpack snapshots.

That means differences should be classified as staged scope decisions, deliberate documented deviations, or alignment gaps. A difference is a current defect when it breaks a semantic Unpack has chosen to provide or a webpack-aligned surface it claims to expose.

## Compiler and compilation lifecycle

Unpack currently runs a compact Rust pipeline with a webpack-shaped sealing boundary:

1. `Compilation::make`
2. `Compilation::seal`
   1. build the chunk graph
   2. assign render IDs
   3. generate one result per module
   4. create assets
3. awaited JavaScript `processAssets`
4. emit assets

Webpack's lifecycle is much larger: `Compiler.run` guards concurrent runs, calls run hooks, reads records, compiles, emits assets, emits records, stores build dependencies, and returns `Stats`. `Compiler.compile` creates normal and context module factories, runs make hooks, finishes the compilation, seals it, and then runs after-compile hooks.

Necessity:

- Keeping Unpack's lifecycle small is a staged implementation choice while the public API grows toward webpack.
- Matching webpack's hook graph, records, cache idle state, and plugin lifecycle becomes necessary when the corresponding public plugin and lifecycle surfaces are supported.
- The phase names remain useful implementation boundaries. Code generation
  results deliberately belong to one `Compilation`; they are not a persistent
  cache boundary.

The Node adapter moves the completed Module Graph into the awaited
`finishModules` callback and requires the callback wrapper to return that graph
before Rust continues into `seal`. This avoids cloning the full graph at the
native seam. Before returning ownership, the JavaScript wrapper materializes
the module wrappers required by the hook, while Module Graph connections cross
the native boundary lazily in per-module batches. Once the awaited hook has
settled, queries from detached JavaScript work that require another native
graph read fail with an expired-lease error while Rust seals instead of reading
a raw pointer concurrently. Purely materialized JavaScript records remain
ordinary values. The final native graph is then rebound to the same JavaScript
`Compilation`; already materialized connection targets are refreshed by handle
and phase-dependent caches are invalidated in place.

Generated assets use the same ownership-handoff pattern at the
`processAssets` boundary. Rust moves the asset vector into a phase-scoped Node
lease after `seal`; JavaScript materializes webpack `Source` values only when a
plugin actually taps the hook, runs taps serially, and returns the updated asset
set before output is written. This keeps asset mutation lock-free and prevents
Rust emission from racing asynchronous plugin work. The supported façade
includes the assets argument, `Compilation.assets`, `emitAsset`, `updateAsset`,
and `getAsset`. Mutations made through the façade before `processAssets` are
staged and merged when generated assets cross the host boundary. Unlike
webpack, emitting different content to an existing filename currently uses the
latest Source without adding a Compilation error; source-map metadata and the
rest of webpack's Asset Info API remain outside this slice.

The root Rust `CompilationHooks` trait is an object-safe host-transport seam
owned by `CompilerOptions`, not an implementation of webpack's individual Hook
classes. It aggregates the Node callbacks needed to pause the compiler at
`compilation`, `finishModules`, and `processAssets`; the public hook objects and
ordering behavior remain on the JavaScript `Compiler` and `Compilation`
facades. This Rust-specific aggregate keeps N-API transport out of the core
Compilation module.

## Cache layout

Unpack's top-level `cache` and `cache_facade` modules map to webpack's root
`Cache.js` and `CacheFacade.js` responsibilities. Memory Cache Plugin, Memory
With GC Cache Plugin, and Pack File Cache Strategy implementations live in
separate modules below the `cache` category. The private `Cache`
module is the Compiler-owned deep module for lifecycle and facade composition;
its private `CacheLayers` helper coordinates ordered storage adapters. Cache
Items, options, and Pack File storage remain Rust-only helpers colocated with
the webpack responsibility that owns them. There is no separate `BuildCache`
type, file, or source hierarchy.

Webpack's `moduleMemCaches` and `moduleMemCaches2` use per-Module
`WeakTupleMap` values. Unpack represents their implemented computations as
typed slots in one Compiler-owned Module Computation Cache. The second-stage
slots are validated after ID Assignment from Render IDs, Chunk membership,
Async Block Chunk references, and Exports Info, and currently have one
effective runtime rather than webpack's Runtime Spec variants.

Webpack wires these responsibilities through plugins and Tapable hooks. Unpack
keeps its existing typed layers, explicit lifecycle methods, and closed Cache
Item families under ADR 0131; the different Rust representation is deliberate,
while the directory and file boundaries remain webpack-locatable.

This layout change covers the cache-category alignment tracked by issue #217.
The first serialization-layout slice moves generic Serializer identity,
registration, type erasure, bounded encoding, and typed decoding into
`serialization/serializer.rs`. Pack File record codecs remain beside their
format-specific DTOs, and binary framing plus index persistence remain in the
private Pack File storage module until later slices establish the corresponding
Binary Middleware and File Middleware boundaries. The private format and codec
behavior do not change during these physical moves.

## Normal module factory

Unpack's `NormalModuleFactory` resolves a dependency request, matches the
optional minimal `module.rules` condition, and builds a `ModuleIdentity`. The
first loader slice supports non-overlapping unflagged regular-expression rules,
absolute CommonJS loaders, JSON options, direct string or Promise returns, and
asynchronous callbacks. Matching modules remain
`JavaScriptAuto`; the loader path participates in module identity, file
dependencies, watch dependencies, and Module Build Record validation.

The public TypeScript package keeps a small `LoaderRuntime` transport helper in
`LoaderRuntime.ts`. Loader functions must execute on the JavaScript thread while
the Normal Module Factory and compilation remain Rust-owned, so this helper
loads and invokes configured CommonJS loaders for the internal N-API bridge.
It is an internal host-transport boundary rather than a separate webpack public
surface or a replacement for the Normal Module Factory responsibility.

Webpack's `NormalModuleFactory` owns a large part of public configurability: hooks, scheme-specific resolution, rule matching, loader resolution, parser/generator selection, layers, import attributes, dependency categories, file/missing/context dependency tracking, and ignored modules.

Necessity:

- The minimal Unpack factory and loader rule schema are staged scope reductions.
- The current `ModuleIdentity` shape is still useful because loaders, layers, module types, and query/fragment behavior can be added without redefining graph identity.
- Webpack's factory hook surface should be introduced when plugin and loader API work starts, using webpack names and ordering as the reference.

## Make phase and errors

Unpack uses `FuturesUnordered` plus a semaphore to factorize, read, parse, and connect modules. Module-attributable make errors are recorded in `Compilation::errors`; infrastructure failures still return `Err` and stop the run.

Webpack uses separate async queues for factorize, add, build, rebuild, and
process-dependencies. Unpack uses Rust-native tasks but now follows the same
completed-compilation boundary: a module-attributable build or code-generation
failure remains in the Module Graph, is reported through Stats, and renders as a
throwing module factory. Unaffected entry code and deferred paths remain usable
until execution reaches that factory.

Necessity:

- Rust-native queues are fine; copying webpack's `AsyncQueue` API is not needed.
- Missing Render IDs and malformed graph connections remain infrastructure
  invariants and terminate compilation instead of being converted into module
  failures.

## Dependency model and parser

Unpack uses a minimal webpack-like dependency set for ESM imports, exports, re-exports, and static-string dynamic imports. It stores normal dependencies, async dependency blocks, and presentational dependencies separately, and code generation applies dependency templates to `rspack_sources` replacement sources.

The closed `Dependency` enum remains at the top-level webpack `Dependency`
boundary. `AsyncDependenciesBlock` and `ModuleGraphConnection` each have their
own top-level modules, while implemented concrete dependency payloads live in
the `dependencies` category with one webpack-corresponding module per upstream
class. Public crate re-exports preserve the existing Rust API; the physical
split is an ownership and navigation boundary rather than a new extension
surface.

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

Unpack keeps the `build_chunk_graph` planning algorithm separate from the
`ChunkGraph` relationship store, matching webpack's `buildChunkGraph` and
`ChunkGraph` responsibility boundary. The implementation uses a dense
`ModuleMask` indexed by Rust `ModuleHandle` values for `min_available_modules` and
`resulting_available_modules` intersections, while retaining ordered
`Vec<ModuleHandle>` traversal results where output order matters. Dense arena
keys use `*Handle` names so webpack's `Module ID` and `Chunk ID` names remain
reserved for generated output identity.

Unpack creates one entrypoint chunk group per entry, assigns statically reachable
modules to the initial chunk, and creates or reuses async chunk groups by dynamic
import target. A terminating worklist recursively discovers nested blocks. Each
Async Chunk plan intersects modules available after every parent loading group,
then emits the target's static closure minus that intersection. Newly discovered
parents can only shrink the intersection and add required factories. A dynamic
import back to a module already available after its parent group creates no Chunk
Group edge and remains a Promise-based require in generated code. When global
target reuse discovers reciprocal cross-import parents, materialization omits a
redundant parent edge that would close a Chunk Group cycle while retaining the
Dependency Block's target mapping. A separate logical runtime-tree adjacency
retains every loading relationship, including the omitted material edge; runtime
requirement traversal is cycle-safe and therefore still reaches both sides for
each Entrypoint. Each Entrypoint keeps its own Runtime Modules and installed chunk
state while requirements from all logically reachable nested groups propagate to
its runtime tree.

Webpack's `buildChunkGraph` is much broader. It tracks runtime per chunk group, chunk loading and async chunk flags, named async chunk reuse, async entrypoints, available-module masks, skipped modules, conditional connections, nested blocks, pre/post order indices, child/parent updates, and block-to-chunk-group links.

Necessity:

- Unpack's chunk group and many-to-many membership model is necessary because it preserves the shape needed for later split chunks and runtime chunks.
- Reusing one Async Chunk plan per target Module is a staged internal deviation;
  webpack's block-first `ChunkGroupInfo` model becomes necessary before named
  async groups, per-block options, or full split-point identity can be claimed.
- Intersecting parent available-module sets is necessary: excluding a module
  seen on only one path would make the shared payload unusable from another.
- Split chunks, cache groups, min-size/min-chunks/max-request rules, runtime chunks, and named chunk options are not necessary for the first implementation.
- Recursive nested-block processing and available-module back-edge collapse are
  required parts of Unpack's implemented dynamic-import semantics.

## Runtime and asset generation

Unpack emits webpack-shaped Node/CommonJS output with a fixed module table,
module cache, core `__webpack_require__`, and CommonJS entry startup. Generated
code declares Runtime Requirements. Their closed set is stored as an inline
`u16` mask rather than a general-purpose tree set, and selected Runtime Modules
are deduplicated in an inline `u8` mask. A fixed-point resolver orders Runtime
Modules for export getters, own-property checks, namespace marking,
chunk ensuring, filename lookup, add-only module-factory exposure, and cohesive
Node require chunk loading. Static-only Bundles omit all asynchronous helpers.
For runtime trees with loadable Async Chunks, the Node loader registers payload
factories, executes optional payload runtime, then marks every payload chunk ID
loaded; load and runtime failures leave installation retryable. Asset emission
and filename lookup share one fixed id-based JavaScript filename resolver.
Source maps remain available for generated assets.

Webpack renders runtime behavior through runtime modules and runtime requirements. Its Node require chunk loading module computes output paths, conditions loading on chunk type, handles installed chunk state, supports optional on-chunk-load hooks, external install hooks, HMR, base URI, and generated filename templates.

Necessity:

- Starting with fixed Node require chunk loading is intentional and covered by ADR `0032`.
- Runtime Requirements drive both static helpers and the implemented Node
  require Chunk Loading Runtime; the legacy monolithic async path is removed.
- Browser JSONP, ESM chunk loading, HMR, public path, chunk filename templates, and external chunk installation are future target features, not current requirements.

The Node-runtime closure coordinates with the API-alignment and benchmark
tracks (#140, #145, and #147). The JavaScript package entry remains ESM-only,
while emitted entry assets intentionally use CommonJS startup. Render IDs are
readable and deterministic with controlled churn, but are not a byte-for-byte
webpack ID contract. Context modules, general CommonJS parsing/interop, broader
loader rules and loader chains, public JavaScript plugins, Inner Graph, module
concatenation, browser loading targets, and other unimplemented options remain
explicit unsupported surfaces. Provided-exports, used-exports, and Side Effects
Flag Plugin responsibilities are implemented for the current ESM surface.

## ESM code generation

Unpack preserves source and applies templates for const replacements, import side effects, import specifier reads, export headers, export specifiers, default exports, re-exports, and dynamic imports. It intentionally uses webpack-shaped names and getter-based export bindings.

Init fragments follow webpack's cyclic-Harmony ordering: compatibility setup runs
first, export getters are installed before ordinary imports execute, and star
re-exports run after their import namespace is available. This keeps early
namespace reads safe while preserving later live-binding updates.

Webpack templates include additional behavior for used exports, inlined exports, export presence diagnostics, CommonJS/default interop, module concatenation, deferred imports, dead branch imports, async modules, namespace object variants, and precise star re-export conflict handling.

Necessity:

- Getter-based export bindings and import-usage rewrites are necessary for webpack-like ESM live binding semantics.
- The current provided-exports and used-exports plugins drive export getter
  emission and side-effects connection optimization for the supported ESM
  surface.
- Inner Graph, broader export-usage semantics, ambiguous star export conflict
  handling, namespace/default interop, and module concatenation should be
  deferred until their corresponding webpack surfaces are chosen.

## Render IDs and filenames

Unpack uses readable module render IDs relative to context and derives async chunk render IDs from target module paths. Webpack assigns module and chunk IDs through configurable id plugins and computes filenames through output templates.

Necessity:

- Readable render IDs are good for early debugging and semantic tests.
- Deterministic production IDs, hashing, and filename templates are future output-stability features.
- Byte-for-byte webpack output is not a goal.

## Current priority classification

Current staged scope limits:

- No broad webpack configuration, loader-chain, plugin, or compilation API parity yet.
- Minimal Rust-native compiler and normal module factory.
- ESM-first parser surface.
- Fixed Node require chunk loading target.
- Readable render IDs and semantic tests.
- Byte-offset source ranges in Rust.

Current documented architecture deviations and resolved violations are tracked
in `webpack-architecture-deviation-register.md`. Nested dynamic imports and
Runtime Requirement-driven helper emission are implemented and are no longer
alignment gaps.

Feature work to defer until explicitly chosen:

- Context modules and non-static dynamic imports.
- CommonJS parsing and interop.
- Broader loader rules, loader chains, loader options, async loaders, and plugin API parity.
- Magic comments, dynamic import modes, import attributes, deferred/source import phases.
- Split chunks, cache groups, runtime chunks, HMR, browser/ESM/webworker chunk loading.
- Inner Graph, module concatenation, broader export-usage semantics, and deterministic id plugins.
