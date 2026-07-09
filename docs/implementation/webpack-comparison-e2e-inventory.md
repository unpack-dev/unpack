# Webpack comparison e2e inventory

This inventory resolves
[Inventory implemented surfaces and current e2e coverage](https://github.com/unpack-dev/unpack/issues/95)
for the wayfinder map
[Add webpack comparison e2e coverage for implemented surfaces](https://github.com/unpack-dev/unpack/issues/94).

## Scope

The confirmed e2e boundary is the public JavaScript API exposed by
`@unpack-js/core`. A comparison test may execute generated bundles, inspect
emitted assets, and compare observable behavior against the repo-managed pinned
`webpack@5.108.1` dependency. Byte-for-byte webpack output matching and broad
webpack test-suite import stay out of scope.

## Current implemented surfaces

The exposed JavaScript API surface currently includes:

- `unpack(options, callback?)`, where `options` accepts `context`, `mode`,
  `entry`, `output.path`, `sourcemap`, `cache`, `snapshot`, and
  `infrastructureLogging`.
- `Compiler.run(callback)`, `Compiler.watch(watchOptions, handler)`, and
  `Compiler.close(callback)`.
- `Watching.invalidate()` and `Watching.close(callback)`.
- `Stats.hasErrors()` and `Stats.toJson()`, including errors, warnings,
  assets, output path, and watch dependency sets.
- `watchOptions.aggregateTimeout`, `watchOptions.ignored`, and
  `watchOptions.poll`.

Implemented bundling and runtime behavior includes:

- Single string entries and object entries.
- Webpack-shaped emitted assets with module tables, module cache,
  `__webpack_require__`, export getters, namespace marking, dynamic import
  chunk loading, and source maps.
- Static ESM imports, side-effect imports, named/default/namespace imports,
  named exports, default exports, named re-exports, simple star re-exports, and
  static-string dynamic imports.
- Webpack-like completed-compilation error reporting for parse and resolve
  errors, including emitted throwing module factories.
- Memory cache, disabled cache, opt-in filesystem cache, mode-aware snapshot
  defaults, managed/immutable/unmanaged path classification, missing input
  snapshots, and context directory snapshots.
- User-facing infrastructure logging through `infrastructureLogging.level`.

## Existing coverage by group

### Public API and lifecycle

| Surface | JavaScript API Test coverage | Webpack Comparison Test coverage | Gap and priority |
| --- | --- | --- | --- |
| `unpack(options)` asset emission and default run path | `api.test.ts` covers emitted assets, default asset names, output path, and webpack runtime names. | None. | Medium. Useful as a smoke comparison, but output/runtime strategy should decide assertion style. |
| Object entries | `api.test.ts` covers object entries and per-entry assets. | None. | Medium. A good output-scope candidate if webpack-compatible entry naming is part of the first expansion. |
| Top-level callback validation timing | Unpack-only validation exists in `api.test.ts`. | `webpack-lifecycle-alignment.test.ts` observes webpack and current Unpack behavior. | Already covered as an observation-style lifecycle gap. |
| Top-level callback timing and returned compiler lifecycle | `api.test.ts` covers Unpack's automatic close behavior. | `webpack-lifecycle-alignment.test.ts` observes webpack and current Unpack behavior. | Already covered as an observation-style lifecycle gap. |
| `compiler.run(callback)` callback timing | `api.test.ts` covers asynchronous callback timing and concurrent run rejection. | `webpack-lifecycle-alignment.test.ts` compares async callback timing. | Partially covered. Concurrent run behavior should be evaluated in the lifecycle expansion ticket. |
| `compiler.run(callback)` `err` versus `Stats` | `api.test.ts` covers parse errors in stats and emitted throwing assets. | `webpack-lifecycle-alignment.test.ts` compares parse-error `err`, `Stats`, and `hasErrors()`. | Partially covered. Resolve-error stats semantics are not compared yet. |
| Manual compiler reuse and close | `api.test.ts` covers rerun, source edits, close, and closed compiler errors. | None. | High. `compiler.close(callback)` rows remain TBD in the lifecycle matrix. |
| `compiler.watch(...)` and `Watching` lifecycle | `api.test.ts` covers initial build, close, invalidate, watched dependency rebuilds, aggregate timeout, ignored paths, polling, and conflicts. | None. | High. Multiple watch rows remain TBD in the lifecycle matrix. |
| `Stats.hasErrors()` and `Stats.toJson()` shape | `api.test.ts` covers errors, assets, output path, and watch dependency sets. | Only parse-error `hasErrors()` is compared. | High. `Stats.toJson()` lifecycle shape remains TBD in the lifecycle matrix. |

### Watch, cache, snapshot, logging, and validation

| Surface | JavaScript API Test coverage | Webpack Comparison Test coverage | Gap and priority |
| --- | --- | --- | --- |
| `mode` and snapshot defaults | `api.test.ts` covers omitted/production versus development/none module and resolve snapshot defaults, plus build dependency defaults. | None. | High. This is explicitly webpack-aligned behavior and should be considered by the cache/snapshot strategy ticket. |
| `cache: false` and memory cache behavior | `api.test.ts` covers disabled cache invalidation and compiler reruns. | None. | Medium. Compare only if webpack has a clean same-scenario observable; otherwise keep as Unpack API regression coverage. |
| Filesystem cache option shape and persistence | `api.test.ts` covers cache shape, manifest/pack writes, idle flush, close flush, and readonly mode. | None. | Low for comparison. The serialized cache schema is Unpack-private, so structure assertions should remain Unpack-only. |
| Managed, immutable, unmanaged, missing, and context snapshot semantics | `api.test.ts` covers package version invalidation, unversioned fallback, unmanaged precedence, immutable bypass, missing candidate invalidation, and context directory invalidation. | None. | High for selected behavior-level comparisons, but not for Unpack-private snapshot storage details. |
| Watch options validation | `api.test.ts` covers unknown keys, `ignored`, and `poll` validation. | None. | Medium. Compare supported subset behavior where webpack's observable semantics are relevant; keep exact TypeError text Unpack-only. |
| Unsupported and unknown option validation | `api.test.ts` covers invalid `mode`, invalid `sourcemap`, `plugins`, infrastructure logging, cache, and snapshot validation. | Only invalid `mode` lifecycle timing is compared. | Medium. Unsupported webpack option rejection is an Implemented Webpack Surface, but exact messages should not be compared. |
| Infrastructure logging | `api.test.ts` covers default quiet behavior, `info`, `verbose`, and exclusion from `Stats`. | None. | Medium. ADR 0083 documents an Unpack default deviation from webpack; strategy should decide whether to record comparison evidence or keep this Unpack-only. |

### Webpack-shaped output and runtime semantics

| Surface | JavaScript API Test coverage | Webpack Comparison Test coverage | Internal/Core coverage | Gap and priority |
| --- | --- | --- | --- | --- |
| Basic emitted asset list and runtime helper shape | `api.test.ts` covers `main.js`, source map assets, output path, and `__webpack_require__`. | None. | `codegen.rs` checks generated helper names and source map references. | High as a comparison smoke test, using behavior/structure assertions rather than byte output. |
| Source map toggle | `api.test.ts` covers `sourcemap: false`. | None. | `codegen.rs` covers disabled source map assets. | Medium. This is an Unpack-specific `sourcemap` option, not webpack `devtool`; compare only if the strategy defines a webpack equivalent. |
| Static ESM graph construction | Limited public API coverage through emitted output. | None. | `make.rs` covers side-effect imports, import specifiers, re-exports, star re-exports, module graph connections, and deduplication. | High. Needs public JavaScript API e2e coverage that executes generated bundles. |
| Dynamic import and async chunks | Public API tests do not execute dynamic imports. | None. | `make.rs` records dynamic split points; `codegen.rs` executes dynamic import chunks through Node. | High. This is a strong candidate for pinned-webpack comparison e2e. |
| Live export bindings and import usage rewrite | Public API tests do not execute live binding scenarios. | None. | `codegen.rs` executes live binding behavior through Node. | High. This should graduate into output/runtime comparison scope. |
| Multi-entry reused async chunks | Public API tests cover object entries but not reused async chunks. | None. | `codegen.rs` executes multi-entry async chunk reuse through Node. | Medium to high. Include if output/runtime strategy wants multi-entry chunk behavior early. |
| Parse and resolve errors as completed compilations | `api.test.ts` covers parse errors in stats and emitted throwing assets. | Parse-error run semantics are compared in lifecycle tests. | `make.rs` covers parse errors, context dynamic import rejection, and resolve errors as failed modules. | High for resolve-error `Stats` and emitted behavior; context-module rejection is out of scope for comparison unless documenting a deliberate deviation. |
| Nested dynamic imports | Not covered by public API tests. | None. | `webpack-implementation-differences.md` identifies current nested async split handling as a parity gap. | High as an implementation gap, but it may need a fix before it can become a shared alignment assertion. |

## First expansion candidates

The highest-value candidates for the first comparison expansion are:

1. Complete the lifecycle comparison matrix rows that are still `TBD`:
   `compiler.close(callback)`, `compiler.watch(...)`, `Watching.invalidate()`,
   `Watching.close(callback)`, and `Stats.toJson()`.
2. Add output/runtime comparison scenarios through the JavaScript API for a
   static ESM bundle, live binding behavior, dynamic import chunk loading, and
   parse/resolve completed-compilation behavior.
3. Add selected cache/snapshot comparison scenarios for mode-aware snapshot
   defaults and invalidation behavior, while keeping persistent cache file
   format assertions Unpack-only.
4. Decide whether infrastructure logging and unsupported-option validation need
   comparison evidence or should remain Unpack-only tests backed by documented
   deviations.

## Surfaces to keep out of the first comparison expansion

- Exact emitted JavaScript text, exact source map text, exact error messages,
  and persistent cache manifest/pack format.
- Internal module graph and chunk graph structure unless the behavior is exposed
  through generated assets or bundle execution.
- Context modules, non-static dynamic imports, loader pipelines, plugin hooks,
  CommonJS parsing, browser JSONP chunk loading, split chunks, and target
  selection.

## Existing test files

- `packages/unpack/test/api.test.ts`: broad Unpack-only JavaScript API Test
  coverage for current public API behavior.
- `packages/unpack/test/webpack-lifecycle-alignment.test.ts`: the only current
  Webpack Comparison Test file, covering three lifecycle observations.
- `crates/unpack_core/tests/make.rs`: internal make/module graph/chunk graph
  coverage.
- `crates/unpack_core/tests/codegen.rs`: internal generated-output and Node
  runtime coverage.
