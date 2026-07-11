# Support a minimal JavaScript loader pipeline

Unpack will expose a narrow webpack-shaped `module.rules` surface so the
Cross-Bundler Benchmark can run its loader fixture through the real Normal
Module build pipeline. The first surface accepts non-overlapping rules containing
an unflagged JavaScript `RegExp` `test`, an absolute CommonJS `loader` path, and
optional JSON-serializable loader options.
Unsupported rule fields and combinations fail during option normalization
rather than becoming no-op webpack compatibility placeholders.

Rule matching occurs in Rust against the resolved absolute resource path. A
matching loader is included in `ModuleIdentity`, becomes a file and cache
dependency, receives UTF-8 source through a Node threadsafe callback, and must
return JavaScript source synchronously as a string. The loader runs with a
minimal context containing `resourcePath`, `rootContext`, `getOptions`,
`sourceMap: false`, and an asynchronous callback returned by `async`. Loader loading,
execution, and return-type failures are module-attributable Compilation errors.

Loader modules are reloaded once per Compilation and reused for every matching
module in that Compilation. Module Build Records cache transformed source and
are invalidated by either the resource or direct loader file changing. The
first implementation does not track the loader's transitive CommonJS
dependencies and does not guarantee execution order between resources.

Loader chains, relative or package loader resolution, overlapping rule composition,
pitch, raw buffers, emitted files, additional loader
dependencies, and source-map composition remain unsupported. Configurations
with loader rules therefore require `sourcemap: false`. These surfaces should
only open when their observable behavior can be represented by the compilation
model rather than by benchmark-specific preprocessing.
