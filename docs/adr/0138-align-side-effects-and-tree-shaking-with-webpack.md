# Align side effects and tree shaking with webpack

Unpack will implement `optimization.sideEffects` and tree shaking by following
webpack's implementation, responsibility boundaries, phase ordering, naming,
and source layout as closely as Rust permits. Behavioral compatibility alone
is not sufficient when webpack already provides a clear internal design.

## Decision

Unpack will introduce a webpack-recognizable `SideEffectsFlagPlugin` as the
owner of side-effects analysis and dependency optimization. Its Rust module
must live at the source-layout equivalent of webpack's
`lib/optimize/SideEffectsFlagPlugin.js`, using Rust filename conventions only.
Related webpack units must remain separately locatable rather than being folded
into Make, Chunk Graph construction, or code generation.

The plugin will follow webpack's lifecycle and responsibilities:

- obtain package `sideEffects` metadata through the normal module factory and
  resolver/package-description path;
- support webpack's boolean and pattern-array package metadata semantics,
  including relative-path and glob handling;
- set module side-effects metadata during module creation instead of reading
  package files while building the Chunk Graph;
- when `optimization.sideEffects` is `true`, use parser hooks to perform
  webpack-equivalent source-level side-effect analysis;
- when it is `"flag"`, consume declared side-effects flags without enabling
  source-level analysis;
- optimize side-effect-free re-export connections during webpack's equivalent
  of `optimize_dependencies`, storing the result on Module Graph connections;
- leave Chunk Graph construction to consume connection state without
  reimplementing side-effects policy.

The existing webpack-shaped provided-exports and used-exports plugins remain
separate plugins and phases. Side-effects analysis may consume their graph
metadata, but it must not merge their responsibilities into one Unpack-only
tree-shaking pass.

Public option normalization will preserve webpack's option values and
mode-dependent defaults. Values that have observably different webpack
semantics must remain distinct in the Rust model rather than being collapsed
to a boolean at the JavaScript/native boundary.

Tests will port the relevant webpack configuration cases and retain their
upstream paths/version in comments or fixture documentation. Important public
behavior will also use the repository's pinned webpack dependency as a
comparison oracle where practical.

## Deviations

A deviation from webpack's implementation, naming, phase boundary, hook
placement, or directory/file organization is permitted only for a concrete
Rust ownership, type-system, concurrency, performance, or build-system
constraint. Each deviation must document:

1. the corresponding webpack implementation and file;
2. the concrete constraint preventing alignment;
3. the narrow alternative chosen by Unpack;
4. tests proving that the deviation preserves webpack-observable behavior.

Convenience, a smaller initial patch, or the absence of an exposed plugin API
is not by itself a sufficient reason to diverge.

## Migration requirement

The initial implementation that reads `package.json` from Make, collapses
`true` and `"flag"`, and decides dependency activity in `build_chunk_graph` is
an alignment gap, not the target architecture. It must be migrated to the
plugin and graph-metadata design above before the feature is considered
complete. Webpack's source analysis and package pattern arrays are required
parts of completion, not optional follow-up features.
