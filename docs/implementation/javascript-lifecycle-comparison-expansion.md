# JavaScript lifecycle comparison expansion

This note resolves
[Decide lifecycle comparison expansion scope](https://github.com/unpack-dev/unpack/issues/97)
for the wayfinder map
[Add webpack comparison e2e coverage for implemented surfaces](https://github.com/unpack-dev/unpack/issues/94).

## Decision

The first lifecycle comparison expansion should cover every remaining `TBD`
row in `docs/implementation/javascript-lifecycle-webpack-alignment.md`, but it
should do so with focused observation-style tests where current Unpack behavior
intentionally differs from webpack's lifecycle behavior.

All lifecycle comparison tests should stay in
`packages/unpack/test/webpack-lifecycle-alignment.test.ts`. Ordinary Unpack
regression tests in `packages/unpack/test/api.test.ts` should not be deleted as
part of the first comparison expansion. After comparison tests land and the
matrix rows are filled, exact duplicate Unpack-only lifecycle assertions can be
deduplicated in a separate cleanup.

## Scenarios to add

### `compiler.close(callback)` lifecycle behavior

Add comparison coverage for:

- Idle `compiler.close(callback)` callback timing and `err`.
- `compiler.run(callback)` after `compiler.close(callback)`.
- `compiler.close(callback)` while a `compiler.run(callback)` is active.

Expected classification:

- Idle close timing differs in current observations: webpack invokes the close
  callback synchronously for the idle compiler case observed with pinned
  webpack, while Unpack invokes it asynchronously. Treat this as an
  observation-style row first; if ADR 0055 remains the intended constraint, the
  matrix should classify the async close timing as a documented webpack
  deviation tied to explicit native/cache cleanup.
- Run after close differs: pinned webpack still accepts a later run after an
  idle close, while Unpack reports `CompilerClosedError` with no `Stats`. ADR
  0055 already documents the Unpack closed-compiler boundary, so classify this
  as a documented webpack deviation unless the project decides to redraw that
  boundary.
- Close while a run is active differs: pinned webpack allows the close callback
  observed in this scenario to complete without an error, while Unpack reports a
  compiler-running infrastructure error. ADR 0055 documents Unpack's rejection
  of close during an active run, so record this as a documented webpack
  deviation.

### `compiler.watch(watchOptions, handler)` initial callback behavior

Add shared-alignment coverage for:

- `compiler.watch({}, handler)` returns a `Watching` handle synchronously.
- The initial handler invocation is asynchronous at observable granularity.
- The initial handler receives `err === null`, a `Stats` object, and
  `stats.hasErrors() === false` for a valid fixture.

Expected classification:

- Aligned for the first expansion's observable timing and stats availability
  boundaries.

### `compiler.watch(watchOptions, handler)` conflict behavior

Add observation-style coverage for:

- `compiler.run(callback)` while a watch session is active.
- Starting a second `compiler.watch(...)` while a watch session is active.
- `compiler.close(callback)` while a watch session is active.

Expected classification:

- Both webpack and Unpack reject concurrent run/watch work on the same compiler,
  but pinned webpack reports these conflict callbacks synchronously with
  webpack's `ConcurrentCompilationError`, while Unpack reports asynchronous
  infrastructure errors with Unpack error names. ADRs 0043, 0053, and 0076
  document Unpack's asynchronous conflict callbacks and per-compiler conflict
  boundary, so use observation-style tests and classify callback timing/error
  taxonomy differences as documented webpack deviations.
- `compiler.close(callback)` during watch differs similarly to close during run:
  pinned webpack allows the close callback observed in this scenario to complete
  without an error, while Unpack reports `CompilerRunningError`. ADR 0076
  documents the Unpack requirement to close the `Watching` handle first, so
  classify this as a documented webpack deviation.

### `Watching.invalidate()` behavior

Add shared-alignment coverage for:

- Calling `watching.invalidate()` after the initial watch build triggers a
  second asynchronous handler invocation.
- The rebuild callback receives `err === null`, a `Stats` object, and
  `stats.hasErrors() === false`.

Expected classification:

- Aligned at the observable lifecycle boundary. Do not assert microtask versus
  macrotask ordering.

### `Watching.close(callback)` behavior

Add comparison coverage for:

- `watching.close(callback)` after an initial watch build.
- Reusing the same compiler with `compiler.run(callback)` after
  `Watching.close(callback)`.

Expected classification:

- Functional behavior is aligned: both close the watch session and allow the
  compiler to run again.
- Callback timing differs in current observations: pinned webpack invokes the
  close callback synchronously for the idle watching case, while Unpack invokes
  it asynchronously. Treat this as an observation-style test and classify the
  timing difference as a documented webpack deviation if the project keeps ADR
  0075's cleanup semantics as the boundary.

### `Stats.hasErrors()` behavior

Add comparison coverage for:

- A successful run or watch build returns `stats.hasErrors() === false`.
- A resolve-error fixture completes with `err === null`, a `Stats` object, and
  `stats.hasErrors() === true`.

Expected classification:

- Success and parse-error `hasErrors()` behavior are already covered or implied
  by current comparison tests.
- Resolve-error behavior should be added because Unpack now records resolve
  errors as completed-compilation diagnostics; this should be a shared
  alignment assertion if pinned webpack observes the same `err` versus `Stats`
  boundary.

### `Stats.toJson()` lifecycle shape

Add comparison coverage for:

- A normalized shared subset: `errors`, `warnings`, emitted asset names, and
  `outputPath` presence.
- A documented shape observation showing webpack's default `stats.toJson()` is
  broader while Unpack intentionally exposes a minimal `StatsJson` with
  `errors`, `warnings`, `assets`, `outputPath`, and `watchDependencies`.

Expected classification:

- Normalized subset behavior should be aligned.
- Full default `toJson()` object shape is a documented webpack deviation under
  ADR 0041. Do not make Unpack match webpack's full stats object before the
  public stats surface is deliberately expanded.

## Existing API tests

Do not mechanically move existing tests out of `api.test.ts`.

Add comparison coverage for these lifecycle assertions, then optionally
deduplicate later:

- `manual compiler remains reusable until close`: compare compiler reuse before
  close, close behavior, and run-after-close behavior, but keep bundle emission
  assertions in `api.test.ts`.
- `watch performs initial build and close keeps compiler reusable`: compare
  initial watch lifecycle, `Watching.close`, and run-after-watch-close behavior.
- `watch invalidate triggers rebuild`: compare `Watching.invalidate()`
  lifecycle behavior.
- `watch conflicts with run watch and compiler close`: compare conflict
  behavior as observation-style tests.
- `compilation errors are reported in stats and still emit assets`: compare
  lifecycle `err`/`Stats`/`hasErrors()` behavior for resolve errors; leave exact
  emitted throwing asset checks to output/runtime scope.

Keep these as ordinary Unpack regression tests:

- Exact Unpack error names and message text.
- Strict top-level option validation and watch option validation, including
  unknown option handling.
- Filesystem cache idle flush and close flush behavior.
- Watch dependency set contents in `Stats.toJson().watchDependencies`.
- Polling and `aggregateTimeout` timing details beyond the observable
  invalidate/rebuild lifecycle.
- Generated asset contents and source edit emission behavior.

## Follow-up implementation tickets to graduate later

The final ticket-creation task should create implementation tickets along these
lines after the output/runtime and cache/snapshot/logging strategy tickets have
also resolved:

- Add compiler close lifecycle comparison tests.
- Add compiler watch and Watching lifecycle comparison tests.
- Add Stats lifecycle comparison tests and update the lifecycle matrix.

Each implementation ticket should update
`docs/implementation/javascript-lifecycle-webpack-alignment.md` in the same
change as the comparison tests that back the newly filled matrix rows.
