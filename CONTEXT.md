# Unpack

Unpack is a JavaScript bundling project that aims for webpack-like outcomes without treating webpack compatibility as part of its product contract.

## Language

**Webpack-like**:
Similar in purpose and workflow to webpack, but free to use its own public API, configuration model, loader model, and plugin model.
_Avoid_: Webpack-compatible

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
Bundle output whose file structure and runtime semantics resemble webpack output, including concepts such as module tables, module cache, entry bundles, and asynchronous chunk loading, without promising byte-for-byte output matching or webpack API compatibility.
_Avoid_: Webpack-compatible output, snapshot-compatible webpack output

**JavaScript API**:
The Node.js-facing programmable API for configuring and running Unpack from JavaScript.
_Avoid_: Rust API, webpack-compatible API

**JavaScript API Test**:
A test authored from the JavaScript side that exercises Unpack through the public JavaScript API boundary.
_Avoid_: Rust core test, internal facade test

**Cross-Bundler Benchmark**:
A performance comparison that runs the same benchmark fixture through Unpack and selected external bundlers. Its results are diagnostic signals, not merge gates or compatibility claims.
_Avoid_: Compatibility benchmark, conformance benchmark, release gate

**Benchmark Fixture**:
A generated application module graph constrained to JavaScript features the compared bundlers are expected to compile. It exists to produce comparable bundle work, not to model a real application.
_Avoid_: Sample app, real-world app, compatibility fixture

**Cold Build Measurement**:
A benchmark measurement taken after clearing the output path and benchmark-owned cache state for the tool under test.
_Avoid_: First run, clean test

**Warm Build Measurement**:
A benchmark measurement taken after a prior build in the same benchmark job while preserving benchmark-owned cache state that the tool under test can reuse.
_Avoid_: Incremental rebuild, watch rebuild

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

**Export Binding**:
A generated runtime binding that exposes an ECMAScript module export through a getter so importers observe the current exported value.
_Avoid_: Export snapshot, CommonJS export assignment

**Exports Info**:
The module graph metadata that records a module's known exports and how generated code should name them.
_Avoid_: Export list, tree shaking table

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
_Avoid_: Compilation cache, module graph reuse

**Cache Facade**:
A scoped access point to the build cache for one compiler subsystem or cache item family.
_Avoid_: Specialized cache method, cache namespace

**Cache Item**:
A named unit of reusable build information stored in the build cache with its own validation data.
_Avoid_: Cache blob, cached compilation part

**Resolve Record**:
A cache item that represents the validated result of resolving one dependency request from one issuer context.
_Avoid_: Resolver cache entry, resolved path cache

**Module Build Record**:
A cache item that represents the validated result of building one module for reuse across compilations.
_Avoid_: Cached module, parsed module cache

**File Snapshot**:
A recorded view of filesystem inputs used to decide whether cached build information is still valid.
_Avoid_: Watch event, mtime check

**Snapshot Strategy**:
The configured validation method for a file snapshot, such as timestamp validation, content-hash validation, or a combination of both.
_Avoid_: Cache mode, watcher policy

**Snapshot Category**:
A class of filesystem inputs that can use its own snapshot strategy, such as module resources, resolution inputs, or build dependencies.
_Avoid_: Cache namespace, watcher group

**Build Dependency**:
A toolchain or configuration input whose change can invalidate persistent build cache entries.
_Avoid_: Application dependency, module dependency

**Persistent Cache**:
A build cache stored outside the process so later compiler instances can reuse validated build information.
_Avoid_: Disk cache, offline cache

**Cache Pack**:
A grouped persistent cache storage unit that holds multiple cache items plus metadata needed to find and validate them.
_Avoid_: Module cache file, cache blob

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
