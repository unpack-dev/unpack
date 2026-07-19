# Support loader module requests through the current Compilation

Loader contexts will expose a first webpack-shaped `loadModule` and
`importModule` slice. Requests resolve relative to the resource whose loader is
running, pass through matching configured module rules, become dependencies of
the requesting Module in the current Compilation, and contribute their direct
resource and loader files to watch dependency sets and Module Build Record
snapshots.

This extends the already Implemented Webpack Surface from ADR 0136 rather than
opening an unrelated feature area. It follows ADR 0116 by closing loader-context
alignment gaps in the existing Normal Module build pipeline.

`loadModule` keeps webpack's callback-only contract and returns transformed
text, a null source map, and a minimal module facade. `importModule` keeps the
Promise and callback overloads and returns an ECMAScript module namespace.
Non-empty execution options and inline loader requests fail clearly instead of
being accepted as no-ops.

The first Build-Time Module Execution slice asks the Rust Make Phase to resolve,
build, cache, and connect every requested Module, then evaluates the resulting
transformed ECMAScript module text on the JavaScript host thread. It does not claim webpack's full
Compilation execution runtime: CommonJS execution, emitted loader assets,
source maps, layers, public paths, base URIs, and arbitrary transformed relative
dependencies remain closed. Opening those surfaces requires a
Compilation-owned code-generation runtime. The Loader Runtime remains an internal
N-API transport and host-execution helper; request resolution, rule matching,
Module Graph ownership, dependency discovery, caching, and snapshots remain in
the Rust Make Phase. Loader requests use dedicated `LoaderDependency` and
`LoaderImportDependency` records and do not become runtime Chunk members.
