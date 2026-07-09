# Webpack watch cache snapshot and logging comparison scope

This note resolves
[Decide watch cache snapshot and logging comparison scope](https://github.com/unpack-dev/unpack/issues/99)
for the wayfinder map
[Add webpack comparison e2e coverage for implemented surfaces](https://github.com/unpack-dev/unpack/issues/94).

## Decision

The first watch/cache/snapshot expansion should add comparison coverage in two
feature areas:

- `packages/unpack/test/webpack-watch-incremental-alignment.test.ts` for watch
  option and incremental rebuild behavior that is not already owned by the
  lifecycle matrix.
- `packages/unpack/test/webpack-cache-snapshot-alignment.test.ts` for selected
  cache and snapshot behavior.

Do not create a first-pass `webpack-infrastructure-logging-alignment.test.ts`.
Infrastructure logging should mostly remain ordinary Unpack JavaScript API
coverage because Unpack's first logging surface is deliberately narrower and
quieter than webpack's.

Use the shared helper shape from
`docs/implementation/webpack-comparison-e2e-harness.md`. Keep lifecycle callback
rows such as `compiler.watch(...)`, `Watching.invalidate()`, and
`Watching.close(callback)` in
`packages/unpack/test/webpack-lifecycle-alignment.test.ts`; this scope covers
watch options and rebuild behavior.

## Watch and incremental rebuild scenarios

### Watched file rebuilds

Add a shared-alignment test where a watch session observes a changed file
dependency and emits updated output.

Assertion style:

- Build the same fixture with pinned webpack and Unpack.
- Wait for the initial handler, mutate a watched dependency, then wait for the
  next handler.
- Compare only observable facts: two successful handler calls, `err === null`,
  `stats.hasErrors() === false`, and updated emitted output.
- Avoid asserting exact watcher internals or event timing.

Expected classification:

- Aligned.

### `aggregateTimeout` coalescing

Add a shared-alignment test where two rapid edits under `aggregateTimeout`
produce one rebuild callback with the final source.

Assertion style:

- Use a modest timeout and explicit post-rebuild delay.
- Compare handler call count at observable granularity and emitted output.
- Do not assert exact milliseconds or filesystem event ordering.

Expected classification:

- Aligned.

### Polling rebuilds

Add a shared-alignment test for `watchOptions.poll` with a numeric interval.

Assertion style:

- Use `aggregateTimeout: 0` and a small numeric `poll` interval.
- Mutate an entry file and compare that each bundler observes a second
  successful callback with updated output.

Expected classification:

- Aligned for numeric polling rebuild behavior.
- Keep `poll: true`'s exact default interval Unpack-only.

### Ignored paths

Add shared-alignment coverage only for a RegExp ignored pattern such as
`/ignored\\.js$/`.

Assertion style:

- Assert a dependency edit under the ignored RegExp does not trigger a second
  watch callback within a bounded wait.
- Use the same RegExp for webpack and Unpack.

Expected classification:

- Aligned for RegExp ignore behavior.
- String/path ignore behavior differs in current observations: pinned webpack
  still rebuilt for `ignored: "ignored.js"` and an absolute-style string, while
  Unpack ignored the same file. Keep string/path ignore behavior in
  `api.test.ts` or record it with an observation-style comparison, not a shared
  assertion.

### Watch dependency sets

Do not add a shared-alignment assertion for exact watch dependency shape in the
first expansion.

Current Unpack exposes `Stats.toJson().watchDependencies` with `files`,
`contexts`, and `missing`. Pinned webpack exposes broader dependency sets on
`stats.compilation.fileDependencies`, `contextDependencies`, and
`missingDependencies`, and does not expose the same minimal shape through the
normalized `stats.toJson()` subset.

If dependency-set evidence is useful, add an observation-style test that shows
both bundlers record the entry file, a resolved dependency, and missing
resolution inputs through their respective public objects. Keep exact path
sets, ancestor directory entries, extension candidates, and `toJson()` shape
Unpack-only.

## Cache and snapshot scenarios

### `cache: false`

Add a shared-alignment test showing disabled cache rebuilds a same-timestamp
source edit.

Assertion style:

- Use one compiler per bundler.
- Run once, edit source content while preserving the timestamp, run again, and
  compare emitted output.

Expected classification:

- Aligned.

### Explicit module hash snapshots

Add a shared-alignment test for explicit `snapshot.module: { timestamp: false,
hash: true }`.

Assertion style:

- Use memory cache.
- Run once, edit source content while preserving the timestamp, run again, and
  compare emitted output.

Expected classification:

- Aligned for explicit hash invalidation.

### Mode-aware module and resolve defaults

Add observation-style tests before writing any shared assertions for mode-aware
snapshot defaults.

Current observations with pinned `webpack@5.108.1` and memory cache:

- For same-timestamp module edits, pinned webpack reused the old output for
  omitted, `production`, `development`, and `none` modes unless explicit module
  hash snapshots were configured.
- Current Unpack invalidated same-timestamp module edits for omitted and
  `production`, but reused old output for `development` and `none`.
- For same-timestamp package `exports` edits, pinned webpack reused the old
  output in the probed omitted/production/development/none cases, while current
  Unpack invalidated omitted/production and reused development/none.

Expected classification:

- Observation-style first. The current Unpack defaults are stronger than the
  pinned webpack behavior observed for the same public API scenario.
- If the project keeps the current omitted/production timestamp-plus-hash
  defaults from ADR 0098, record that as a documented webpack deviation.
  Otherwise, graduate a follow-up fix to align defaults with pinned webpack.

### Missing resolver candidates

Add a shared-alignment test for missing resolve candidates appearing on disk.

Assertion style:

- Build `import "./dep"` when only `dep.js` exists.
- Add a higher-priority extension candidate such as `dep.ts`.
- Run again with cache enabled and compare emitted output.

Expected classification:

- Aligned.

### Context directory and managed path invalidation

Do not add shared assertions for context directory or managed path behavior in
the first expansion.

Current observations showed differences:

- A package-main context directory change with a preserved `package.json`
  timestamp invalidated in Unpack but reused old output in pinned webpack.
- A managed `node_modules` package source edit invalidated in pinned webpack
  but not in Unpack until `package.json` version changed.

Expected classification:

- Observation-style only if comparison evidence is needed.
- Keep exact managed item modeling, immutable bypass, unmanaged precedence,
  hidden/scoped package handling, and context directory digest behavior
  Unpack-only until the project decides whether these stronger invalidation
  semantics are deliberate documented deviations or alignment gaps.

### Build dependency and filesystem cache behavior

Do not make filesystem cache persistence a shared webpack comparison target in
the first expansion.

Current observations showed pinned webpack reusing old output for a
same-timestamp build dependency edit in the probed filesystem-cache scenario,
while Unpack invalidated through its build-dependency hash snapshot. This is
useful evidence, but it should be recorded observation-style rather than as a
shared assertion.

Keep these Unpack-only:

- `cacheDirectory`, `cacheLocation`, `name`, `version`, `readonly`,
  `idleTimeout`, and `maxMemoryGenerations` exact behavior.
- Persistent cache manifest and pack paths, JSON/CBOR structure, magic values,
  schema details, idle flush timing, and close-flush guarantees.
- Readonly persistent cache restore/write behavior.
- Build-dependency and resolve-build-dependency serialized snapshot contents.

## Option validation

Do not broadly compare exact option-validation messages.

Add observation-style validation tests only where a documented compatibility
boundary matters:

- `snapshot.contextModule` is accepted by webpack's schema but rejected by
  Unpack until context modules exist.
- Snapshot strategies with both `timestamp` and `hash` disabled are accepted by
  webpack's schema but rejected by Unpack to avoid permanent cache entries.

Keep these as Unpack-only validation tests:

- Exact `TypeError` names and messages.
- Unknown option rejection for Unpack's strict wrapper.
- Unsupported `cache` keys and unsupported `infrastructureLogging` keys.
- Relative snapshot path string rejection and unsupported RegExp flag/Rust regex
  rejection, except as observation evidence if a later ticket needs it.
- Watch option validation details for the narrow supported subset.

## Infrastructure logging

Keep infrastructure logging behavior in ordinary Unpack JavaScript API tests for
the first comparison expansion.

Pinned webpack produced no console output in the probed Node API scenarios for
default, `info`, or `verbose` infrastructure logging levels. Current Unpack is
quiet by default, but at `info` and `verbose` it emits explicit console messages
owned by ADR 0083.

Comparison guidance:

- A low-value shared smoke test may assert that the default JavaScript API run
  is quiet and that logs are not added to the minimal stats JSON shape.
- Do not compare `info` or `verbose` event names, logger names, console methods,
  exact ordering, or message text against webpack.
- If logging evidence is needed, use an observation-style test showing pinned
  webpack's API run remains quiet while Unpack emits its documented events.

## Existing tests to keep

Do not mechanically move existing tests out of `packages/unpack/test/api.test.ts`.

Add comparison coverage for these existing behaviors, then deduplicate only if a
later cleanup finds exact duplicate Unpack-only assertions:

- `cache false disables module build cache reuse`: compare `cache: false`.
- `snapshot module hash detects same-timestamp source edits`: compare explicit
  module hash snapshots.
- `missing resolver candidates appearing invalidate resolve records`: compare
  missing candidate invalidation.
- `watch rebuilds when a watched dependency changes`: compare watched-file
  rebuilds.
- `watch aggregateTimeout coalesces rapid changes`: compare observable
  coalescing.
- `watch poll option rebuilds through polling`: compare numeric polling.
- `watch ignored string prevents rebuilds from ignored files`: keep string form
  Unpack-only; add shared RegExp ignored coverage instead.

Keep these Unpack-only unless a later implementation ticket explicitly asks for
observation evidence:

- Mode-aware omitted/production snapshot defaults.
- Managed, immutable, unmanaged, context directory, and build-dependency
  invalidation details.
- `Stats.toJson().watchDependencies` exact shape.
- Filesystem cache files, idle flush, close flush, readonly behavior, and cache
  schema details.
- Infrastructure logging `info` and `verbose` messages.
- Strict validation message text and unsupported-option taxonomy.

## Follow-up implementation tickets to graduate later

The final ticket-creation task should create implementation tickets along these
lines:

- Add watch incremental comparison tests for watched-file rebuilds,
  `aggregateTimeout`, numeric polling, and RegExp ignored paths.
- Add cache/snapshot comparison tests for `cache: false`, explicit module hash
  snapshots, and missing resolver candidate invalidation.
- Add observation-style cache/snapshot tests for mode-aware defaults and the
  selected documented-deviation candidates.
- Keep infrastructure logging as Unpack-only coverage unless the final plan
  explicitly wants a low-value default-quiet smoke comparison.

Each implementation ticket should keep the suite green on current behavior.
Shared-alignment scenarios should use normalized shared assertions; known
differences should use observation-style tests and link back to the decision
that classifies them.
