# Unpack

Unpack is a JavaScript bundling project that explores the performance ceiling achievable while aligning as closely as possible with webpack's architecture and functionality.

## Language

**Webpack-like**:
Aligned with webpack's purpose, workflow, public JavaScript API shape, configuration concepts, loader model, plugin model, naming, and internal compilation flow where practical. Deviations from webpack should be deliberate and documented.
_Avoid_: Loosely webpack-inspired, Unpack-only API by default

**Bundle**:
The emitted JavaScript and related assets produced from an application's dependency graph.
_Avoid_: Pack, build output

**Asset**:
A named output item produced by a compilation, such as a generated JavaScript file, before it is written to disk.
_Avoid_: File output, artifact

**Output Path**:
The filesystem directory where a compiler run writes emitted assets for JavaScript API users.
_Avoid_: Dist directory, build folder

**Webpack-shaped Output**:
Bundle output whose file structure and runtime semantics resemble webpack output, including concepts such as module tables, module cache, entry bundles, and asynchronous chunk loading. Byte-for-byte output matching is not required unless a test or feature explicitly chooses it.
_Avoid_: Unpack-specific output, snapshot-compatible webpack output

**JavaScript API**:
The Node.js-facing programmable API for configuring and running Unpack from JavaScript; it should follow webpack's public API shape and option names where practical.
_Avoid_: Rust API, Unpack-only API

**Exposed JavaScript API Surface**:
The currently callable JavaScript API behavior, including `unpack(options, callback?)`, configured JavaScript Plugins, compiler lifecycle methods, watching lifecycle methods, stats reporting, and supported option normalization. It is the first priority for webpack alignment before generated runtime or graph internals.
_Avoid_: Internal Rust API, unimplemented plugin surface

**JavaScript Plugin**:
A webpack-shaped object with an `apply(compiler)` method or a function-style plugin configured through `options.plugins`. It is applied once in configuration order and can use only the model-backed Compiler and Compilation Hooks and façade properties that Unpack exposes.
_Avoid_: Rust plugin, arbitrary webpack plugin compatibility

**JavaScript Lifecycle Alignment**:
The webpack-aligned behavior of JavaScript API calls around synchronous validation, asynchronous callback timing, callback `err` values, returned `Stats`, and compiler or watching lifecycle conflicts. It is the first exposed API alignment area to stabilize.
_Avoid_: Runtime code alignment, graph alignment

**Lifecycle Alignment Matrix**:
A comparison document for JavaScript lifecycle alignment that records each API scenario's webpack behavior, current Unpack behavior, classification, required tests, and required fixes. It should be produced before changing lifecycle behavior.
_Avoid_: Ad hoc bug list, implementation-only TODO list

**Documented Webpack Deviation**:
A deliberate difference from webpack behavior that is recorded with its reason and boundary. Unrecorded differences in implemented webpack surfaces should be treated as alignment gaps to fix.
_Avoid_: Accidental divergence, undocumented incompatibility

**Webpack API Alignment**:
The expectation that each exposed webpack-shaped JavaScript API or option should match webpack's call shape, defaults, error timing, callback semantics, and main observable behavior. Unimplemented webpack surfaces should fail loudly or be documented as alignment gaps rather than silently diverging.
_Avoid_: Byte-for-byte webpack output, full webpack test-suite parity

**Unsupported Webpack Option**:
A webpack-supported JavaScript API option that Unpack recognizes as part of the webpack surface but has not implemented yet. It should produce a clear validation error instead of being ignored or accepted as a no-op.
_Avoid_: Ignored option, placeholder option

**Model-Backed Webpack Surface**:
A webpack-shaped public API surface that is exposed only after Unpack's internal compilation model can support its observable behavior. It prevents public options, hooks, loaders, or plugin entrypoints from becoming no-op compatibility placeholders.
_Avoid_: API-first compatibility, no-op webpack surface

**Implemented Webpack Surface**:
A webpack-shaped behavior, API, option, runtime helper, or internal compilation concept that Unpack already exposes or relies on. These surfaces should be aligned with webpack before adding new webpack feature areas.
_Avoid_: Future feature surface, proposed webpack surface

**Webpack Internal Alignment**:
The expectation that Unpack's internal bundler concepts use webpack names, phase ordering, responsibility boundaries, and source-layout boundaries where practical, while allowing Rust-native traits, enums, ownership, concurrency, dense handles, indexed storage, and bit sets. Implemented webpack units should map to the corresponding directory level and separate file boundary unless a documented Rust constraint requires otherwise. JavaScript object shapes, class inheritance, and hook storage should only be copied when they affect exposed plugin or loader behavior.
_Avoid_: Rust-only terminology, unrelated source layout, copying webpack JavaScript classes by default

**Mode**:
A JavaScript API option that selects a webpack-like default behavior profile, such as development, production, or none.
_Avoid_: Environment variable, target, optimization preset

**JavaScript API Test**:
A test authored from the JavaScript side that exercises Unpack through the public JavaScript API boundary.
_Avoid_: Rust core test, internal facade test

**Webpack Comparison Test**:
A JavaScript API test that runs the same observable scenario against webpack and Unpack to verify behavior-level alignment. It is used for important exposed API behavior, not for byte-for-byte output matching or wholesale webpack test-suite parity.
_Avoid_: Byte-for-byte snapshot test, full webpack conformance suite

**Executable Webpack Reference**:
A repo-managed, pinned webpack dependency used by comparison tests as the source of observable webpack behavior. Local webpack checkouts may help explain implementation details, but they should not be the required test oracle.
_Avoid_: Personal webpack checkout, ad hoc installed webpack

**Cross-Bundler Benchmark**:
A performance comparison that runs the same benchmark fixture through Unpack and selected external bundlers. Its results are diagnostic signals, not merge gates or compatibility claims.
_Avoid_: Compatibility benchmark, conformance benchmark, release gate

**Benchmark Fixture**:
A generated or locally materialized application module graph constrained to JavaScript features the compared bundlers are expected to compile. It exists to produce comparable bundle work, not to model a real application.
_Avoid_: Sample app, real-world app, compatibility fixture

**Cold Build Measurement**:
A benchmark measurement taken after clearing the output path and benchmark-owned cache state for the tool under test.
_Avoid_: First run, clean test

**Warm Build Measurement**:
A benchmark measurement taken after a prior build in the same benchmark job while preserving benchmark-owned cache state that the tool under test can reuse.
_Avoid_: Incremental rebuild, watch rebuild

**Watch Build Measurement**:
A development-mode benchmark measurement of a rebuild within one Watch Lifecycle, with Memory Cache enabled and Persistent Cache disabled. The initial compilation is excluded; measurement begins immediately before the benchmark fixture mutation and ends when the resulting compilation completes.
_Avoid_: Warm build, initial watch compilation

**Tracing**:
Internal execution signals captured for Unpack developers while maintaining or debugging the bundler itself.
_Avoid_: User logging, stats logging, telemetry

**Logging**:
User-facing build messages exposed through the JavaScript API and controlled by user configuration.
_Avoid_: Internal tracing, Rust debug output, diagnostics

**Infrastructure Logging**:
User-facing logging for compiler infrastructure activity, configured through the JavaScript API without adding log entries to `Stats`.
_Avoid_: Stats logging, compilation logger, tracing

**Infrastructure Log Event**:
A user-facing infrastructure logging message with a level, logger name, and message text.
_Avoid_: Trace event, stats entry, diagnostic

**Module Graph**:
The connected set of modules reachable from one or more entry points.
_Avoid_: Dependency tree

**Graph Handle**:
An opaque, dense Rust index used to address a Module, Chunk, Chunk Group, or connection in compilation-owned storage. Handle names must not use webpack's Module ID or Chunk ID terms, which refer to generated output identity.
_Avoid_: Internal Module ID, internal Chunk ID, Render ID

**Chunk Graph**:
The derived graph that assigns modules from the module graph to initial and async chunks before bundle code is generated.
_Avoid_: Module graph chunks, output graph

**Chunk Group**:
A loading relationship that groups one or more chunks and connects parent and child loading paths.
_Avoid_: Chunk collection, chunk set

**Entrypoint**:
An initial chunk group created from a configured entry.
_Avoid_: Entry chunk, main chunk

**Make Phase**:
The compilation stage that starts from entry modules, discovers their dependencies, and constructs the module graph before bundling.
_Avoid_: Build phase, parse phase

**Code Generation Phase**:
The compilation stage that turns chunks and their modules into webpack-shaped output files.
_Avoid_: Emit phase, print phase

**Source-Preserving Rewrite**:
A code generation approach that keeps a module's original source as the base text and applies dependency-driven replacements and insertions to produce bundle output.
_Avoid_: AST reprint, regex replacement

**Chunk Loading Runtime**:
The generated bundle helper code that loads async chunks and makes their modules available to dynamic imports.
_Avoid_: Browser loader, script loader

**Runtime Requirement**:
A generated-code dependency on a runtime helper that asset creation must provide for a module or chunk.
_Avoid_: Runtime flag, helper import

**Runtime Module**:
A named, stage-ordered generator for one runtime helper selected by the closed set of Runtime Requirements. Runtime Modules are deduplicated per Entrypoint runtime tree and rendered only when required.
_Avoid_: Monolithic runtime block, arbitrary helper snippet

**Code Generation Result**:
The per-module result of the Code Generation Phase: source-preserving rewritten module content plus its direct Runtime Requirements and any attributable generation error.
_Avoid_: Asset, rendered bundle, compilation result

**ID Assignment**:
The deterministic phase that assigns readable named Render IDs to modules and chunks, using stable identities and collision handling before code generation.
_Avoid_: Filename generation, incidental map index

**Export Binding**:
A generated runtime binding that exposes an ECMAScript module export through a getter so importers observe the current exported value.
_Avoid_: Export snapshot, CommonJS export assignment

**Exports Info**:
The module graph metadata that records a module's known exports and how generated code should name them.
_Avoid_: Export list, tree shaking table

**Side Effects Flag Plugin**:
The webpack-aligned optimization plugin that records whether modules may be skipped for evaluation and optimizes side-effect-free dependency connections.
_Avoid_: Chunk graph side-effects check, Make-time package probe

**Module Side Effects State**:
Module Graph metadata describing whether evaluating a module may have observable effects, derived from declared package or rule metadata and, when enabled, source analysis.
_Avoid_: Used export state, module reachability flag

**Static ESM Dependency**:
A dependency declared by an ECMAScript module import or re-export whose specifier is known before code execution.
_Avoid_: Runtime import, CommonJS dependency

**Dependency Template**:
A code generation rule associated with a dependency that rewrites the relevant source segment into webpack-shaped runtime code.
_Avoid_: String patch, codegen callback

**Init Fragment**:
A generated code fragment inserted around a module's rewritten source to initialize export bindings or other module-level runtime state.
_Avoid_: Template prefix, runtime snippet

**Dynamic Import Dependency**:
A dependency declared by an ECMAScript `import()` expression whose specifier is known before code execution.
_Avoid_: Lazy import, runtime import

**Async Split Point**:
A module graph edge introduced by a dynamic import dependency where the target module should belong to asynchronously loaded bundle output.
_Avoid_: Lazy boundary, chunk trigger

**Nested Async Split Point**:
An async split point encountered within code that is itself loaded asynchronously.
_Avoid_: Second-level dynamic import, nested lazy import

**Async Dependencies Block**:
A dependency block created for asynchronously loaded dependencies, such as a static-string dynamic import, that acts as the chunk graph's split-point input.
_Avoid_: Dynamic import dependency marker, lazy import group

**Initial Chunk**:
Bundle output that must be loaded before an entry can run.
_Avoid_: Entry bundle, startup file

**Async Chunk**:
Bundle output loaded on demand because execution reaches an async split point.
_Avoid_: Lazy chunk, dynamic import bundle

**Chunk Render ID**:
The chunk key used to name generated async chunk files and reference them from runtime chunk-loading code.
_Avoid_: Chunk filename, chunk index

**Context Module**:
The bundler concept for a set of possible modules selected by a runtime expression rather than by one static dependency specifier.
_Avoid_: Dynamic import dependency, wildcard import

**Compiler**:
The long-lived bundler object that owns configuration and creates compilations.
_Avoid_: Builder, runner

**Compiler Close**:
The JavaScript API lifecycle operation that releases resources owned by a compiler instance after callers are finished running it.
_Avoid_: Dispose, destroy

**Compilation**:
A single bundling attempt with its own module graph and build-time state.
_Avoid_: Build run, compiler instance

**Watch Session**:
A long-lived compiler lifecycle that observes input changes and triggers compilations from compiler-owned state.
_Avoid_: Watch mode, dev server loop

**Watching**:
The JavaScript API handle returned from starting a watch session.
_Avoid_: Watch session, watcher instance

**Watch Options**:
The JavaScript API options that control file watching and rebuild coalescing for a watch session.
_Avoid_: Compiler options, cache options

**Watch Dependency Set**:
The compilation-reported filesystem inputs that a watch session uses to subscribe to future changes.
_Avoid_: Module dependency, import dependency

**Cache Options**:
The JavaScript API options that enable, disable, and configure build cache layers.
_Avoid_: Watch options, output options

**Snapshot Options**:
The JavaScript API options that configure snapshot strategies by snapshot category.
_Avoid_: Cache options, watch options

**Build Cache**:
Compiler-owned reusable build information that can be validated and reused across compilations.
_Avoid_: Runtime module cache, compilation cache, module graph reuse

**Cache Layer**:
An ordered source of Build Cache data, such as compiler-owned Memory Cache or cross-process Persistent Cache.
_Avoid_: Cache backend, cache implementation

**Memory Cache**:
A Build Cache layer held within one Compiler process for reuse during that Compiler's lifetime.
_Avoid_: Persistent Cache, module graph reuse

**Cache Facade**:
A scoped access point to the build cache for one compiler subsystem or cache item family.
_Avoid_: Specialized cache method, cache store

**Cache Namespace**:
The stable scope contributed by a Cache Facade to distinguish Cache Items owned by different compiler subsystems.
_Avoid_: Cache Facade, cache directory

**Cache Identifier**:
The stable identity of one Cache Item within a Cache Namespace, independent of whether its inputs are current.
_Avoid_: Cache ETag, file path

**Cache ETag**:
A validation token representing the inputs relevant to one Cache Identifier; Cache Items without an ETag use record-level validation data instead.
_Avoid_: Cache Identifier, Snapshot

**Cache Item**:
A named unit of reusable build information stored in the build cache with its own validation data.
_Avoid_: Cache blob, cached compilation part

**Resolve Record**:
A cache item that represents the validated result of resolving one dependency request from one issuer context.
_Avoid_: Resolver cache entry, resolved path cache

**Module Build Record**:
A cache item that represents the validated result of building one module for reuse across compilations.
_Avoid_: Cached module, parsed module cache

**Code Generation Record**:
A Cache Item containing reusable generated module code for one module and runtime input identity.
_Avoid_: Generated bundle, cached Compilation

**Asset Render Record**:
A Cache Item containing reusable rendered source for one Asset identity without representing the Compilation's full Asset lifecycle.
_Avoid_: Cached Asset, cached Compilation

**File Snapshot**:
A recorded view of filesystem inputs used to decide whether cached build information is still valid.
_Avoid_: Watch event, mtime check

**Snapshot**:
A validation record created by File System Info that can contain file, context, missing, managed, and immutable filesystem inputs.
_Avoid_: Watch dependency set, cache item, filesystem cache

**Context Snapshot**:
A recorded view of a filesystem directory context used to decide whether directory-sensitive cached build information is still valid.
_Avoid_: Context module snapshot, directory cache, folder watch event

**Snapshot Merge**:
The File System Info operation that combines two file snapshots into one validation record while preserving each snapshot content category.
_Avoid_: Manifest merge, cache pack merge, dependency concatenation

**Missing Existence Snapshot**:
A recorded absence check for a filesystem input where cache validation only needs to know whether that path has appeared.
_Avoid_: Missing file timestamp, missing dependency error

**File System Info**:
Filesystem metadata service used to create and validate file snapshots with shared path classification and timestamp/hash caching for a compilation or persistent cache backend.
_Avoid_: Filesystem wrapper, snapshot record, watcher cache

**Snapshot Strategy**:
The configured validation method for a file snapshot, such as timestamp validation, content-hash validation, or a combination of both.
_Avoid_: Cache mode, watcher policy

**Snapshot Category**:
A class of filesystem inputs that can use its own snapshot strategy, such as module resources, resolution inputs, or build dependencies.
_Avoid_: Cache namespace, watcher group

**Managed Path**:
A filesystem path whose contents are assumed to be controlled by a package manager and stable unless the package-managed item changes.
_Avoid_: Vendor path, ignored path, external dependency path

**Immutable Path**:
A filesystem path whose contents are assumed not to change because the path includes versioned or content-addressed identity.
_Avoid_: Read-only path, permanent path, static path

**Unmanaged Path**:
A filesystem path that must not use managed-path or immutable-path assumptions when validating file snapshots.
_Avoid_: Source path, dirty path, watched path

**Snapshot Path Pattern**:
A string path or regular expression used by snapshot options to classify filesystem inputs as managed, immutable, or unmanaged.
_Avoid_: JavaScript RegExp compatibility promise, watch ignore pattern

**Build Dependency**:
A toolchain or configuration input whose change can invalidate persistent build cache entries.
_Avoid_: Application dependency, module dependency

**Build Dependency Snapshot**:
A Snapshot of resolved toolchain or configuration inputs used to decide whether a Persistent Cache Container remains valid.
_Avoid_: Resolve Build Dependency Snapshot, module snapshot

**Resolve Build Dependency Snapshot**:
A file snapshot of the resolution work needed to find configured build dependencies before deciding whether persistent cache entries can be reused.
_Avoid_: Build dependency snapshot, resolver cache entry

**Persistent Cache**:
A build cache stored outside the process so later compiler instances can reuse validated build information.
_Avoid_: Disk cache, offline cache

**Persistent Cache Container**:
The restorable unit of Persistent Cache data governed by one cache location, version, and Build Dependency validation boundary.
_Avoid_: Cache Pack, cache directory

**Cache Pack**:
A grouped persistent cache storage unit that holds multiple cache items plus metadata needed to find and validate them.
_Avoid_: Module cache file, cache blob

**Memory Cache Generation**:
One completed Compilation step used to age unused Memory Cache entries.
_Avoid_: Persistent Cache generation, compiler run count

**Cache Idle Flush**:
The lifecycle step that writes pending persistent cache updates after a compiler has no active compilation work.
_Avoid_: Immediate disk write, cache sync

**Stats**:
The JavaScript API report object returned after a compiler run, exposing build results without exposing the compilation's mutable build-time state.
_Avoid_: Compilation result, build report

**Infrastructure Error**:
An error that prevents a compiler run from completing as a bundling attempt.
_Avoid_: Build error, compilation error, fatal error

**Concurrent Run Error**:
An infrastructure error reported when a JavaScript API caller starts a compiler run while the same compiler instance is already running.
_Avoid_: Compilation error, duplicate build warning

**Compiler Running Error**:
An infrastructure error reported when a JavaScript API caller tries to close a compiler while it owns active run or watch work.
_Avoid_: Close error, watcher close error

**Compilation Error**:
A problem found while processing application modules during a completed compiler run.
_Avoid_: Infrastructure error, thrown error

**Failed Module**:
A module that remains in the module graph after module processing reports a compilation error, so emitted output can throw if runtime execution reaches that module.
_Avoid_: Missing module, skipped module

**Module Identity**:
The canonical key used during the make phase to decide whether two resolved module requests refer to the same module instance.
_Avoid_: Module path, file path id

**Module Render ID**:
The module key written into generated bundle output for runtime lookup and debugging.
_Avoid_: Module identity, module index

**Resolver**:
The component that turns a dependency specifier and issuer directory into a resolved module resource.
_Avoid_: Path joiner, import parser

**Normal Module Factory**:
The component that factorizes module dependencies into normal modules by resolving requests and creating module identities.
_Avoid_: Module builder, resolver wrapper
