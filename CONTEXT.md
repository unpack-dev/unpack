# Unpack

Unpack is a JavaScript bundling project that aims for webpack-like outcomes without treating webpack compatibility as part of its product contract.

## Language

**Webpack-like**:
Similar in purpose and workflow to webpack, but free to use its own public API, configuration model, loader model, and plugin model.
_Avoid_: Webpack-compatible

**Bundle**:
The emitted JavaScript and related assets produced from an application's dependency graph.
_Avoid_: Pack, build output

**Module Graph**:
The connected set of modules reachable from one or more entry points.
_Avoid_: Dependency tree

**Make Phase**:
The compilation stage that starts from entry modules, discovers their dependencies, and constructs the module graph before bundling.
_Avoid_: Build phase, parse phase

**Static ESM Dependency**:
A dependency declared by an ECMAScript module import or re-export whose specifier is known before code execution.
_Avoid_: Runtime import, CommonJS dependency

**Dynamic Import Dependency**:
A dependency declared by an ECMAScript `import()` expression whose specifier is known before code execution.
_Avoid_: Lazy import, runtime import

**Async Split Point**:
A module graph edge introduced by a dynamic import dependency where the target module should belong to asynchronously loaded bundle output.
_Avoid_: Lazy boundary, chunk trigger

**Context Module**:
The bundler concept for a set of possible modules selected by a runtime expression rather than by one static dependency specifier.
_Avoid_: Dynamic import dependency, wildcard import

**Compiler**:
The long-lived bundler object that owns configuration and creates compilations.
_Avoid_: Builder, runner

**Compilation**:
A single bundling attempt with its own module graph and build-time state.
_Avoid_: Build run, compiler instance

**Module Identity**:
The canonical key used during the make phase to decide whether two resolved module requests refer to the same module instance.
_Avoid_: Module path, file path id

**Resolver**:
The component that turns a dependency specifier and issuer directory into a resolved module resource.
_Avoid_: Path joiner, import parser
