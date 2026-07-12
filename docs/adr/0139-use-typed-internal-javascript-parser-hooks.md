# Use typed internal JavaScript parser hooks

Unpack will support multiple JavaScript source analyses through crate-private,
typed parser hooks. The hooks are an internal Rust composition seam and will
not be exposed through the JavaScript API or model webpack's general Tapable
runtime.

## Decision

The JavaScript parser will parse each module once and run registered analyses
against that shared AST, comments, and parser-owned analysis services. The
initial phases are `program`, `statement`, and `finish`:

- `program` and `finish` receive the parsed SWC `Module`;
- `statement` receives a statement-scoped parser value that owns comment-range
  bookkeeping and exposes typed analysis queries such as purity;
- analyses write owned results to JavaScript Build Meta or dependency records,
  so parser-owned AST lifetimes do not escape into the Module Graph.

Webpack-recognizable plugins remain responsible for policy. For example,
`SideEffectsFlagPlugin` registers the pure-analysis taps and writes the
side-effect-free Build Meta; the parser only supplies traversal and analysis
services. Harmony dependency collection uses the same parse session rather
than becoming a second analysis pass over reparsed source.

Each tap must declare a stable cache key covering its semantic version and
captured configuration. The ordered phase, tap name, and tap cache key form the
Module Build Cache ETag. Adding, removing, reordering, reconfiguring, or
changing an analysis therefore cannot restore a Module Build Record produced
by an incompatible parser plan.

## Boundaries

This hook set is deliberately smaller than webpack's public parser-hook
surface. More typed phases or parser services should be added only when a
concrete internal analysis needs them. JavaScript plugin exposure would be a
separate public-API decision and must not force the internal Rust hooks to copy
Tapable callback storage, dynamic values, or bail semantics prematurely.

Inner Graph and other future tree-shaking analyses may register on this seam,
but this decision does not implement their semantics.

## Current hook-placement deviation

Webpack's `SideEffectsFlagPlugin` reaches JavaScript parsers through
`NormalModuleFactory.hooks.parser`, and parser instances own the corresponding
hooks. Unpack currently creates the parser after loader transformation inside
concurrent Make Build Tasks; its `NormalModuleFactory` is limited to
factorization and does not own parser construction. Moving parser ownership
there now would either cross the asynchronous loader boundary or split one
immutable analysis plan across factory and build-task state.

As a narrow Rust ownership alternative, internal plugins register an immutable
`JavascriptParserHookSet` on `CompilationHookSet`. The Compilation clones that
plan into Make services, and each Build Task applies it to its parser session.
The hooks remain parser responsibilities: Make transports the plan but contains
no analysis policy, and `SideEffectsFlagPlugin` owns its taps and Build Meta.

The parser phase-order test proves that all taps observe one parse result. The
JavaScript side-effects comparison tests prove the `true`/`"flag"` behavior,
and the filesystem-cache test proves that distinct parser plans do not restore
each other's Module Build Records. If Normal Module Factory later owns parser
creation, registration should move to the webpack-equivalent boundary without
changing the typed hook callbacks or cache contract.
