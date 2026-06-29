# JavaScript lifecycle webpack alignment

This matrix compares webpack behavior with current Unpack behavior for exposed JavaScript lifecycle scenarios before lifecycle fixes are implemented. Each row should be filled from direct webpack comparison tests or code-backed Unpack observations, then classified before implementation work starts.

Callback timing comparisons should first distinguish synchronous throws, synchronous return, and asynchronous callback invocation. They should not lock tests to microtask, `process.nextTick`, or macrotask ordering unless a specific webpack behavior has user-visible semantics that require that precision.

Webpack behavior should come from a repo-managed, pinned webpack dependency used by comparison tests. Local webpack source checkouts may be used to understand implementation details, but the matrix should not depend on a personal checkout path as the behavior oracle.

Matrix rows should be backed by committed webpack comparison tests. One-off exploration scripts may help design those tests, but they should not be the source of record for matrix conclusions.

Lifecycle comparison tests should live in `packages/unpack/test/webpack-lifecycle-alignment.test.ts` so webpack-alignment scenarios stay separate from ordinary API regression tests in `packages/unpack/test/api.test.ts`.

When a scenario reveals a current difference, the first committed test may be an observation-style passing test that asserts webpack's behavior and current Unpack behavior separately while the matrix classifies the difference as an alignment gap. The fix should then tighten that scenario into a shared alignment assertion once Unpack behavior has changed.

| API scenario | Webpack behavior | Current Unpack behavior | Classification | Required test | Required fix |
| --- | --- | --- | --- | --- | --- |
| `unpack(options, callback?)` synchronous validation | TBD | TBD | TBD | TBD | TBD |
| `unpack(options, callback?)` callback timing | TBD | TBD | TBD | TBD | TBD |
| `compiler.run(callback)` callback timing | TBD | TBD | TBD | TBD | TBD |
| `compiler.run(callback)` `err` versus `Stats` | TBD | TBD | TBD | TBD | TBD |
| `compiler.close(callback)` lifecycle behavior | TBD | TBD | TBD | TBD | TBD |
| `compiler.watch(watchOptions, handler)` initial callback behavior | TBD | TBD | TBD | TBD | TBD |
| `compiler.watch(watchOptions, handler)` conflict behavior | TBD | TBD | TBD | TBD | TBD |
| `Watching.invalidate()` behavior | TBD | TBD | TBD | TBD | TBD |
| `Watching.close(callback)` behavior | TBD | TBD | TBD | TBD | TBD |
| `Stats.hasErrors()` behavior | TBD | TBD | TBD | TBD | TBD |
| `Stats.toJson()` lifecycle shape | TBD | TBD | TBD | TBD | TBD |
