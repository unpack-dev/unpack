# Webpack comparison e2e observations

This note records observation-style comparison tests that intentionally keep
the suite green while webpack and current Unpack behavior differ. Shared
alignment tests should use normalized shared assertions; the cases below should
stay observation-style until the linked decision or implementation gap changes.

## Lifecycle behavior

`packages/unpack/test/webpack-lifecycle-alignment.test.ts` records current
compiler close, watch conflict, `Watching.close(callback)`, and default
`Stats.toJson()` shape differences. The canonical row-by-row classification is
`docs/implementation/javascript-lifecycle-webpack-alignment.md`.

- Compiler close and watch conflict timing/error taxonomy remain current
  alignment gaps against the exposed JavaScript lifecycle surface described by
  ADR 0117. Unpack's named infrastructure errors are still documented by ADR
  0060.
- `Stats.toJson()` aligns on the stable subset used by comparison tests:
  `errors`, `warnings`, emitted asset names, and `outputPath`. Broader default
  payload differences are covered by ADR 0041's minimal JavaScript Stats
  surface.

## Output/runtime behavior

`packages/unpack/test/webpack-output-alignment.test.ts` records nested dynamic
imports as an executable output gap. Webpack executes a dynamic import reached
from an async chunk; current Unpack may fail until nested async split points are
implemented.

This is an implementation gap, not a deliberate deviation. ADR 0058 chooses
nested async split points as intended Unpack semantics, and
`docs/implementation/webpack-implementation-differences.md` lists nested
dynamic imports under current gaps to resolve. The observation test should be
converted to a shared runtime assertion when async blocks discovered inside
async chunks create their own chunk groups.

## Cache/snapshot behavior

`packages/unpack/test/webpack-cache-snapshot-alignment.test.ts` records selected
snapshot differences from the pinned webpack reference.

- Mode-aware `snapshot.module` and `snapshot.resolve` default comparisons stay
  observation-style because current Unpack invalidates more strongly than the
  pinned webpack behavior exercised by these same-timestamp fixtures. ADR 0097
  and ADR 0098 document Unpack's chosen mode-aware default model; changing those
  assertions requires an explicit decision to narrow or reinterpret that model.
- `snapshot.contextModule` remains rejected by Unpack until context modules
  exist. This is documented by ADR 0101 and by the snapshot alignment plan in
  `docs/implementation/snapshot-build-cache-alignment.md`.
- Snapshot strategies with both `timestamp` and `hash` disabled remain rejected
  by Unpack to avoid permanently valid cache entries. The same snapshot
  alignment plan records that validation boundary.
