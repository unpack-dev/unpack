# Close Node runtime alignment with evidence

The Node require runtime alignment program is complete for the implemented
webpack-shaped surface. Runtime Requirements select a closed, stage-ordered set
of Runtime Modules per Entrypoint runtime tree; code generation returns direct
requirements and source-preserving module output; and deterministic ID
Assignment gives modules and async chunks readable named Render IDs with stable
collision handling. The old speculative initial-chunk requirement calculation
and monolithic optional-runtime generator are not part of the implementation.

This decision tightens the earlier conservative runtime and first-slice Chunk
Graph decisions in ADRs `0021`, `0014`, `0023`, and `0032` without rewriting
those historical records. Recursive nested async groups, available-module
intersections, cycle-safe logical runtime adjacency, and cyclic Harmony export
initialization are now implemented as documented follow-on behavior in ADRs
`0058` and this closure record.

The first output target remains Node/CommonJS chunk loading. The package entry
point is intentionally ESM-only (ADR `0044`), while emitted entry assets use
CommonJS startup; this is a documented deviation, not a new public option.
Render ID churn is controlled through stable ModuleIdentity ordering and named
collision handling, but byte-for-byte webpack output is not promised. Context
modules, CommonJS parsing/interop, browser chunk loading, loaders, plugins,
tree shaking, and module concatenation remain unsupported surfaces.

Verification is repository-wide and diagnostic: Rust, JavaScript API,
webpack-derived fixtures, source maps, deterministic-output tests, and lint/type
checks are required; cross-bundler benchmarks record evidence but are not merge
gates. The runtime work coordinates with the existing API-alignment and
benchmark tracks (#140, #145, and #147) rather than introducing another public
configuration surface.

Representative measurements recorded during this closure (2026-07-10):

| Measurement | Result |
| --- | ---: |
| Static/async deterministic codegen fixture, initial asset | 3,166 bytes |
| Static/async deterministic codegen fixture, async asset | 398 bytes |
| Cross-bundler `large`, Unpack cold / warm / no-cache | 710.220 / 422.874 / 211.910 ms |
| Cross-bundler `large`, Unpack emitted assets | 436,459 bytes |
| Cross-bundler `large`, webpack 5.108.1 emitted assets | 384,549 bytes |

The deterministic fixture's pre-modular runtime was a monolithic helper block;
the current post-modular result is the recorded 3,166-byte asset and the
runtime helper set is selected by requirements. Historical byte-for-byte
webpack output is intentionally not used as a compatibility gate.
