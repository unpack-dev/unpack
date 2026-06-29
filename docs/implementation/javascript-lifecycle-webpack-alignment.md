# JavaScript lifecycle webpack alignment

This matrix compares webpack behavior with current Unpack behavior for exposed JavaScript lifecycle scenarios before lifecycle fixes are implemented. Each row should be filled from direct webpack comparison tests or code-backed Unpack observations, then classified before implementation work starts.

Callback timing comparisons should first distinguish synchronous throws, synchronous return, and asynchronous callback invocation. They should not lock tests to microtask, `process.nextTick`, or macrotask ordering unless a specific webpack behavior has user-visible semantics that require that precision.

Webpack behavior should come from a repo-managed, pinned webpack dependency used by comparison tests. Local webpack source checkouts may be used to understand implementation details, but the matrix should not depend on a personal checkout path as the behavior oracle.

Matrix rows should be backed by committed webpack comparison tests. One-off exploration scripts may help design those tests, but they should not be the source of record for matrix conclusions.

Lifecycle comparison tests should live in `packages/unpack/test/webpack-lifecycle-alignment.test.ts` so webpack-alignment scenarios stay separate from ordinary API regression tests in `packages/unpack/test/api.test.ts`.

When a scenario reveals a current difference, the first committed test may be an observation-style passing test that asserts webpack's behavior and current Unpack behavior separately while the matrix classifies the difference as an alignment gap. The fix should then tighten that scenario into a shared alignment assertion once Unpack behavior has changed.

| API scenario | Webpack behavior | Current Unpack behavior | Classification | Required test | Required fix |
| --- | --- | --- | --- | --- | --- |
| `unpack(options, callback?)` validation error timing | Invalid `mode` without a callback throws synchronously with `ValidationError`; invalid `mode` with a callback returns no compiler and invokes the callback asynchronously with `ValidationError` and no `Stats`. | Invalid `mode` without or with a callback throws synchronously with `TypeError`; callback-entry validation does not invoke the callback and does not return a compiler. | Alignment gap for callback-entry validation timing and error taxonomy. | `packages/unpack/test/webpack-lifecycle-alignment.test.ts` observes no-callback and callback validation behavior. | Align callback-entry validation errors with webpack or record a narrower documented deviation. |
| `unpack(options, callback?)` callback timing | Returns a compiler synchronously; callback is asynchronous with `err === null` and `Stats`; returned compiler remains runnable after the callback. | Returns a compiler synchronously; callback is asynchronous with `err === null` and `Stats`; returned compiler is closed after the callback. | Alignment gap for returned compiler lifecycle; callback timing aligned. | `packages/unpack/test/webpack-lifecycle-alignment.test.ts` observes callback timing and rerun behavior. | Revisit automatic close from callback entry or document it as a webpack deviation. |
| `compiler.run(callback)` callback timing | `run` returns synchronously and invokes callback asynchronously. | `run` returns synchronously and invokes callback asynchronously. | Aligned. | `packages/unpack/test/webpack-lifecycle-alignment.test.ts` observes parse-error run timing. | None. |
| `compiler.run(callback)` `err` versus `Stats` | Parse errors complete with `err === null`, a `Stats` object, and `stats.hasErrors() === true`. | Parse errors complete with `err === null`, a `Stats` object, and `stats.hasErrors() === true`. | Aligned. | `packages/unpack/test/webpack-lifecycle-alignment.test.ts` observes parse-error stats semantics. | None. |
| `compiler.close(callback)` lifecycle behavior | TBD | TBD | TBD | TBD | TBD |
| `compiler.watch(watchOptions, handler)` initial callback behavior | TBD | TBD | TBD | TBD | TBD |
| `compiler.watch(watchOptions, handler)` conflict behavior | TBD | TBD | TBD | TBD | TBD |
| `Watching.invalidate()` behavior | TBD | TBD | TBD | TBD | TBD |
| `Watching.close(callback)` behavior | TBD | TBD | TBD | TBD | TBD |
| `Stats.hasErrors()` behavior | TBD | TBD | TBD | TBD | TBD |
| `Stats.toJson()` lifecycle shape | TBD | TBD | TBD | TBD | TBD |
