# Webpack output and runtime comparison scope

This note resolves
[Decide output and runtime semantic comparison scope](https://github.com/unpack-dev/unpack/issues/98)
for the wayfinder map
[Add webpack comparison e2e coverage for implemented surfaces](https://github.com/unpack-dev/unpack/issues/94).

## Decision

The first output/runtime comparison expansion should add
`packages/unpack/test/webpack-output-alignment.test.ts`, backed by the shared
helper shape defined in
`docs/implementation/webpack-comparison-e2e-harness.md`.

These tests should compare observable behavior against pinned `webpack@5.108.1`
without byte-for-byte output matching. Runtime behavior should be compared by
executing generated bundles with Node. Asset and runtime shape should be
checked only through stable structural facts such as asset presence, entry
names, source map presence, and webpack-shaped helper names.

Use `mode: "none"` for both bundlers. Use webpack `target: "node"` because
Unpack's current runtime target is fixed to Node/CommonJS require chunk loading.
When a scenario requires `require("./main.js")` or `require("./a.js")` to
expose entry exports, configure webpack with `output.library.type` set to
`"commonjs2"`; Unpack's current initial asset already exports the entry module
through `module.exports`.

For runtime execution scenarios, disable source maps on both sides with webpack
`devtool: false` and Unpack `sourcemap: false` unless source map behavior is
the subject of the scenario. This keeps runtime asset lists small and avoids
conflating executable semantics with source map structure.

## Scenarios to add

### Static ESM executable bundle

Add a shared-alignment test that builds and executes a fixture containing:

- Side-effect imports.
- Named imports and default imports.
- Namespace imports.
- Local named exports and default exports.
- Named re-exports and simple star re-exports.
- A live binding update observed through an importer.

Assertion style:

- Compare normalized build observations: `err === null`,
  `stats.hasErrors() === false`, and entry asset presence.
- Execute both generated `main.js` assets with `process.execPath`.
- Compare stdout from the same script, not generated source text.

Expected classification:

- Aligned for the currently implemented ESM dependency set.
- Keep ambiguous star re-export conflicts, CommonJS interop, export presence
  modes, used-export pruning, and namespace/default interop variants out of the
  first comparison expansion.

### Basic webpack-shaped output smoke

Add a structural smoke test for the same static ESM fixture or a smaller one.

Assertion style:

- Assert both builds emit a requireable `main.js`.
- Assert generated entry assets contain the stable helper vocabulary relevant
  to ESM output, such as `__webpack_require__`, `__webpack_require__.d`, and
  `__webpack_require__.r`.
- Do not compare whole files, module table formatting, helper bodies, module
  id values, bootstrap wrappers, or exact comments.

Expected classification:

- Aligned at the intended webpack-shaped output vocabulary boundary.
- Full runtime module layout remains out of scope.

### Object entries and entry asset names

Add comparison coverage for object entries such as entries named `a` and `b`
pointing at `./src/a.js` and `./src/b.js`.

Assertion style:

- Compare entry asset presence for `a.js` and `b.js`.
- Execute both entry assets and compare stdout for their exported values.
- Do not require exact source map names unless the source map scenario is under
  test.

Expected classification:

- Aligned for current object-entry output.

### Dynamic import and async chunk loading

Add a shared-alignment test for a static-string `import("./feature")` fixture
that returns values from eager modules, the async module, static dependencies
inside the async module, and simple re-exports from the async module.

Assertion style:

- Execute `require("./main.js").loadFeature()` for both bundlers and compare
  stdout.
- Assert each build emits `main.js` and at least one non-entry JavaScript asset.
- Do not compare async chunk filenames or exact async chunk counts. Pinned
  webpack may emit numeric chunk names such as `1.js`, while Unpack currently
  emits readable names such as `src_feature_js.js`.

Expected classification:

- Aligned for executable dynamic import semantics.
- Async chunk render ids and filename templates are staged output-stability
  work, not first-expansion comparison requirements.

### Multi-entry async chunk behavior

Add comparison coverage for two entries that both dynamically import a shared
feature module.

Assertion style:

- Execute both entry assets and compare the combined stdout.
- Assert entry asset presence.
- Avoid asserting exact async chunk count or shared-module factoring. Current
  observations show webpack and Unpack can produce different async asset
  layouts while returning the same runtime values.

Expected classification:

- Aligned at the executable bundle behavior boundary.
- Exact chunk graph factoring and async asset layout remain structural
  implementation details until a later split-chunks or output-stability effort
  chooses them as public comparison boundaries.

### Nested dynamic imports

Add an observation-style test for a dynamic import inside an async chunk.

Assertion style:

- Execute the same fixture in webpack and Unpack.
- Assert pinned webpack successfully resolves the nested import.
- Assert current Unpack behavior separately until fixed.

Expected classification:

- Alignment gap. ADR 0058 says nested async split points are an intended
  semantic, and `docs/implementation/webpack-implementation-differences.md`
  already identifies current nested async chunk handling as a parity gap.
- Convert this to a shared-alignment runtime execution test after Unpack creates
  chunk groups for async blocks discovered inside async chunks.

### Source map asset shape

Add source map comparison coverage with webpack `devtool: "source-map"` and
Unpack's default `sourcemap: true`.

Assertion style:

- Compare asset presence for `main.js` and `main.js.map`.
- Parse each source map and assert stable essentials: `version === 3`,
  `file === "main.js"`, `sources` is non-empty, and `sourcesContent` is
  present.
- Do not compare exact `sources`, mappings, source root, generated positions,
  or webpack runtime-module source entries.

Also add or keep coverage that webpack `devtool: false` and Unpack
`sourcemap: false` omit source map assets when source map behavior is the
scenario under test.

Expected classification:

- Aligned for source map asset presence and minimal map structure.
- Unpack's narrow `sourcemap?: boolean` API and default source map emission are
  documented project choices; do not try to align the full webpack `devtool`
  surface in this expansion.

### Failed module output behavior

Add comparison coverage for parse errors and resolve errors that complete the
compilation with diagnostics.

Assertion style:

- Compare `err === null`, `stats` presence, `stats.hasErrors() === true`, and a
  non-empty normalized error list.
- Assert `main.js` is emitted.
- Execute `require("./main.js")` and assert it throws on the failed module path.
- Do not compare exact error messages, loader suggestions, absolute paths, or
  stack traces.

Expected classification:

- Aligned for completed-compilation output behavior.
- Error text remains Unpack-only or webpack-only observation detail.

## Existing tests to keep

Do not mechanically move existing tests out of `packages/unpack/test/api.test.ts`
or `crates/unpack_core/tests/codegen.rs`.

Add comparison coverage for these existing behaviors, then deduplicate only if a
later cleanup finds exact duplicate Unpack-only assertions:

- `emits assets through the ESM default API`: compare as the basic output smoke.
- `supports object entries`: compare entry asset names and executable exports.
- `can disable sourcemap emission`: compare source map omission against webpack
  `devtool: false` only for asset presence.
- `compilation errors are reported in stats and still emit assets`: compare
  parse-error and resolve-error completed-compilation output semantics.
- `preserves_import_live_bindings` in `codegen.rs`: graduate the runtime
  behavior into the static ESM executable bundle comparison.
- `emits_node_require_chunks_for_dynamic_import` in `codegen.rs`: graduate the
  runtime behavior into the dynamic import comparison.
- `reused_async_chunk_contains_modules_needed_by_each_entry` in `codegen.rs`:
  graduate the executable behavior into the multi-entry async comparison.

Keep these as Unpack-only or Rust-core tests:

- Exact generated source text, helper body text, comments, module render ids,
  readable async chunk filenames, and source map mappings.
- Internal module graph, chunk graph, chunk group, and `RuntimeRequirements`
  structure.
- Source-preserving replacement details and byte-offset source range behavior.
- Unpack-private cache or snapshot interactions that happen to affect emitted
  assets.
- Context modules, non-static dynamic imports, dynamic import magic comments,
  import modes, browser JSONP chunk loading, ESM chunk loading, CommonJS
  parsing, loader pipelines, plugin hooks, split chunks, runtime chunks, HMR,
  deterministic ids, and filename templates.

## Follow-up implementation tickets to graduate later

The final ticket-creation task should create implementation tickets along these
lines after the cache/snapshot/logging strategy ticket also resolves:

- Add the shared non-lifecycle webpack comparison helper.
- Add output/runtime comparison tests for static ESM, object entries, dynamic
  import, source maps, and failed-module output behavior.
- Add an observation-style nested dynamic import comparison test and record the
  current alignment gap.

Each implementation ticket should keep comparison tests green on the current
branch. Shared-alignment scenarios should use normalized shared assertions;
known differences should use observation-style tests until the underlying
alignment fix lands.
