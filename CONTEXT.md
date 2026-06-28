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

**Webpack-shaped Output**:
Bundle output whose file structure and runtime semantics resemble webpack output, including concepts such as module tables, module cache, entry bundles, and asynchronous chunk loading, without promising byte-for-byte output matching or webpack API compatibility.
_Avoid_: Webpack-compatible output, snapshot-compatible webpack output

**JavaScript API**:
The Node.js-facing programmable API for configuring and running Unpack from JavaScript.
_Avoid_: Rust API, webpack-compatible API

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

**Compilation**:
A single bundling attempt with its own module graph and build-time state.
_Avoid_: Build run, compiler instance

**Stats**:
The JavaScript API report object returned after a compiler run, exposing build results without exposing the compilation's mutable build-time state.
_Avoid_: Compilation result, build report

**Infrastructure Error**:
An error that prevents a compiler run from completing as a bundling attempt.
_Avoid_: Build error, compilation error

**Compilation Error**:
A problem found while processing application modules during a completed compiler run.
_Avoid_: Infrastructure error, thrown error

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
