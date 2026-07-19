# Webpack architecture deviation register

This register is the source of record for audits under ADR 0141. It records
webpack architecture differences that require later refactoring, distinguishes
approved deviations from violations, and keeps resolved violations visible so
performance work does not reintroduce them.

The initial audit was performed on 2026-07-12 at commit `1af9791`, using the
repository-pinned `webpack@5.108.1` for observable behavior and webpack source
commit `da91761ed92c8e133ee321c7db4ad6c4698cae0a` for architecture and source
layout. Future architecture-changing performance work must update this register
before implementation and link the explicit project agreement that authorizes
the change.

## Classification

- **Violation**: a performance-driven implementation changes webpack's
  architectural responsibilities, boundaries, naming, or compilation flow
  without explicit agreement before implementation.
- **Confirmed deviation**: the implementation differs from webpack, but its
  constraint, boundary, and approval are documented. It remains a refactoring
  candidate, not a violation.
- **Alignment gap**: a claimed webpack-shaped surface does not yet match the
  corresponding architecture or behavior. Staged unsupported functionality is
  not automatically an alignment gap.
- **Reviewed non-violation**: a Rust-native representation or concurrency
  technique preserves webpack's responsibilities and observable functionality.
- **Resolved violation**: a violating implementation existed historically but
  is absent from the audited revision.

## Current violations

None found in the initial audit. This means no current implementation was shown
to satisfy all three violation conditions; it does not mean Unpack has no
documented deviations or staged webpack scope.

## Confirmed deviations and refactoring triggers

### DEV-001: Async Chunk plans are reused by target Module

- **Status**: Confirmed deviation.
- **Performance-driven**: No. This is staged code-splitting scope.
- **Webpack shape**: webpack starts from each Async Dependencies Block and keeps
  block-first `ChunkGroupInfo` identity.
- **Current shape**: Unpack reuses one Async Chunk plan per target Module while
  retaining each Dependency Block mapping and logical runtime-tree edge.
- **Confirmation**: ADR 0058 and
  `docs/implementation/webpack-implementation-differences.md` document the
  boundary.
- **Refactor before**: named async groups, per-block chunk options, or full
  split-point identity are implemented. Preserve the terminating worklist,
  parent available-module intersections, nested split points, and cycle-safe
  runtime traversal during the refactor.

### DEV-002: JavaScript parser hooks are registered through Compilation

- **Status**: Confirmed deviation.
- **Performance-driven**: No. The constraint is current Rust ownership across
  the asynchronous loader boundary.
- **Webpack shape**: `SideEffectsFlagPlugin` reaches parser instances through
  `NormalModuleFactory.hooks.parser`.
- **Current shape**: plugins register an immutable
  `JavascriptParserHookSet` on `CompilationHookSet`; Compilation transports the
  plan through Make to each parser session.
- **Confirmation**: ADR 0140 documents the ownership constraint, narrow
  alternative, cache-key contract, and required tests.
- **Refactor when**: Normal Module Factory owns parser creation or a public
  parser-hook surface is introduced. Move registration to the webpack-equivalent
  boundary without moving analysis policy into Make.

### DEV-003: Cache lifecycle and serialization use typed Rust seams

- **Status**: Confirmed deviation.
- **Performance-driven**: No. The constraints are Rust typing, ownership, and
  staged cache-plugin exposure.
- **Webpack shape**: Cache behavior is composed through Tapable hooks, and
  PackFile serialization is divided into reusable serializer and middleware
  responsibilities.
- **Current shape**: Compiler-owned `Cache` uses explicit lifecycle methods and
  typed Cache Layers. The reusable Serializer is separate, while binary framing
  and file/index persistence remain private Pack File responsibilities.
- **Confirmation**: ADR 0131 and
  `docs/implementation/webpack-implementation-differences.md` document these
  boundaries.
- **Refactor when**: public cache plugin hooks, Binary Middleware, or File
  Middleware become implemented webpack surfaces. Do not reintroduce a separate
  `BuildCache`, whole-Compilation cache, or whole-project fingerprint during the
  refactor.

### DEV-004: Watch Change Sets may unsafely replace Snapshot validation

- **Status**: Confirmed deviation.
- **Performance-driven**: Yes. Make profiling shows that one changed file still
  replays full-graph Cache lookup, path processing, record reconstruction, and
  graph insertion work.
- **Webpack shape**: webpack carries modified and removed files through its
  Watch lifecycle while File System Info and Snapshots remain part of cache
  validity; Unpack's safe default follows that responsibility.
- **Approved shape**: the explicit
  `experiments.unsafeWatchCacheInvalidation: true` option may trust a model-backed
  Watch Change Set for same-Compiler Memory Cache reuse, bypassing ordinary
  Cache lookup and Snapshot validation for inputs outside that set.
- **Confirmation**: ADR 0142 records the explicit project agreement, unsafe
  correctness trade-off, supported Watch Change Set categories, and fallback
  boundaries required by ADR 0141.
- **Disable or refactor when**: the watch adapter cannot provide a usable change
  set, a rebuild is manually invalidated, reuse crosses a Compiler or process
  boundary, or Persistent Cache is restored. Those paths must retain Snapshot
  validation; the unsafe experiment must never silently become the default.

### DEV-005: Rebuild Make task spawning is configurable

- **Status**: Confirmed deviation.
- **Performance-driven**: Yes. Profiling rebuild Make requires separating Tokio
  task-spawn overhead from Factorize and Build work.
- **Webpack shape**: webpack schedules Make work through its asynchronous queues;
  it does not expose a public scheduler-selection option. Unpack normally wraps
  background Make futures in Tokio tasks while preserving webpack's Factorize,
  Add, Build, and Process Dependencies responsibilities.
- **Approved shape**: ADR 0143 makes direct polling of rebuild Factorize and
  Build futures in Make's `FuturesUnordered` queue the default and permits an
  explicit `experiments.serialRebuildMake: false` to restore Tokio spawning.
  Initial compilations retain Tokio task spawning. Make parallelism is unbounded;
  ADR 0144 removes the former Rust-only finite parallelism setting.
- **Disable or refactor when**: direct polling changes Make phase ordering,
  error delivery, lifecycle behavior, or task responsibilities; the experiment
  must remain configurable unless measurements and a later decision remove one
  of the scheduler paths.

### DEV-006: Concatenated Modules retain Root handles and guarded scopes

- **Status**: Confirmed deviation.
- **Performance-driven**: No. The constraints are the frozen dense Module
  Handle arena and missing resolved top-level binding metadata.
- **Webpack shape**: `ModuleConcatenationPlugin` allocates a synthetic
  `ConcatenatedModule`, replaces the root Module and its connections, and
  directly flattens renamed module scopes.
- **Current shape**: Unpack preserves the Concatenation Root's handle, records
  the `ConcatenatedModule` plan on the Chunk Graph, disconnects Inner Modules
  from affected Chunks, and emits guarded initializer scopes inside the Root's
  single module-table factory.
- **Confirmation**: ADR 0148 and
  `docs/implementation/webpack-implementation-differences.md` document the
  dense-handle and Code Generation boundaries. Public webpack comparison tests
  cover Chunk membership, live bindings, cycles, import order, re-exports,
  cross-Chunk bailouts, generated-name collision avoidance, source maps, and
  rebuild invalidation.
- **Refactor when**: optimization-created Modules can receive stable
  compilation-local handles or the parser retains complete resolved top-level
  symbol/reference metadata. Preserve the plugin phase, candidate selection,
  Original Sources, Runtime Requirements, and cyclic live-binding behavior.

## Resolved violations and alignment gaps

### RES-001: Whole-Compilation warm cache shortcut

- **Status**: Resolved violation.
- **Performance-driven**: Yes.
- **Introduced by**: [PR #76](https://github.com/unpack-dev/unpack/pull/76).
- **Violation**: readonly warm builds could restore final Assets and watch
  dependencies from a cached Compilation record without running Make, rebuilding
  Module Graph and Chunk Graph, or running Code Generation. This replaced the
  webpack-aligned compilation flow for benchmark performance and contradicted
  ADR 0064's existing Cache Item boundary.
- **Resolution**: [PR #81](https://github.com/unpack-dev/unpack/pull/81) removed
  the shortcut. Each run now creates a fresh Compilation and graph while Resolve,
  Module Build, Code Generation, Asset Render, and unaffected-module computations
  are reused through their separate webpack-shaped boundaries.
- **Regression guard**: the Rust compiler tests assert that repeated and aged
  filesystem-cache runs reuse records without sharing a Compilation Graph; the
  JavaScript persistent-cache contract asserts per-item restoration and
  invalidation.

### RES-002: Side-effects policy was placed in Make and Chunk Graph

- **Status**: Resolved alignment gap; not an ADR 0141 violation because the
  motivating change was feature implementation rather than performance.
- **Previous shape**: Make read package metadata, `true` and `"flag"` were
  collapsed, and `build_chunk_graph` decided side-effects connection activity.
- **Resolution**: ADR 0138 required and the current implementation provides a
  webpack-recognizable `SideEffectsFlagPlugin`, distinct provided-exports and
  used-exports plugins, Module Graph connection state, and parser-hook analysis.
- **Regression guard**: JavaScript comparison tests cover option distinctions,
  package patterns, pure analysis, re-export redirection, and rule overrides.

### RES-003: Cache was organized as a separate BuildCache architecture

- **Status**: Resolved alignment gap; not an ADR 0141 violation because no
  unconfirmed performance-driven replacement was identified.
- **Previous shape**: reusable cache responsibilities lived under a separate
  `BuildCache` type and source hierarchy.
- **Resolution**: PR #224 aligned the cache source layout and PR #228
  consolidated ownership into the Compiler-owned webpack `Cache` responsibility.
  ADR 0131 records the current Cache, Cache Facade, Cache Layer, and Cache Item
  boundaries.

## Reviewed non-violations

The initial audit explicitly reviewed the following performance-relevant
techniques. They do not require separate confirmation while their listed
webpack responsibility remains intact:

- `FuturesUnordered`, Tokio tasks, sharded maps, and atomics in
  Make and Cache preserve factorize, add, build, process-dependencies, Cache,
  and Cache Facade responsibilities.
- dense `ModuleHandle` storage, `IndexVec`, and `ModuleMask` preserve separate
  Module Graph, Chunk Graph, and `buildChunkGraph` responsibilities; webpack's
  Module ID and Chunk ID terms remain reserved for generated output identity.
- inline Runtime Requirement and Runtime Module masks preserve named Runtime
  Requirements, Runtime Modules, stages, prerequisite closure, and generated
  runtime behavior.
- Compiler-owned Module Computation Cache preserves webpack's unaffected-module
  memoization boundary and remains separate from record-oriented and persistent
  Cache Layers under ADR 0139.

If a future optimization removes or merges one of those responsibilities, it
must be reclassified as a proposed deviation and confirmed before implementation.
