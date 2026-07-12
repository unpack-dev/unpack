# AGENTS.md instructions

## Project orientation

Unpack is an experimental Rust-based JavaScript bundler whose goal is to
explore the performance ceiling while aligning as closely as practical with
webpack's architecture and functionality. Alignment applies to webpack's
observable behavior, public JavaScript API, option names, lifecycle semantics,
and internal responsibility boundaries. Do not trade away an implemented
webpack surface solely for a simpler or faster Unpack-specific design.

This is a single-context repository. Before exploring or changing behavior:

1. Read root `CONTEXT.md` and use its canonical domain terms.
2. Read the ADRs under `docs/adr/` that govern the area being changed.
3. Check `docs/implementation/webpack-implementation-differences.md` when the
   change touches a webpack-shaped surface.

If a proposed change conflicts with an ADR, call out the conflict rather than
silently overriding the decision. See `docs/agents/domain.md` for the complete
domain-documentation policy.

## Workspace map and ownership

- `crates/unpack_core/`: Rust compiler, compilation state, graphs, caching,
  parsing, and code generation.
- `crates/unpack_node/`: internal napi-rs bridge. Keep native transport details
  out of the public API.
- `packages/unpack/`: public ESM TypeScript package (`@unpack-js/core`), option
  normalization, JavaScript lifecycle behavior, and public API tests.
- `packages/benchmarks/`: diagnostic cross-bundler benchmark runner and
  fixtures. Benchmark results are not compatibility claims or merge gates.
- `docs/adr/`: accepted architecture decisions.
- `docs/implementation/`: implementation plans, differences, and alignment
  notes.

Respect the boundary between the TypeScript public API, the N-API transport,
and the Rust core. Public JavaScript behavior belongs in `packages/unpack/src`;
the binding should remain internal; compiler behavior and domain state belong
in `crates/unpack_core`.

Generated outputs are `target/`, `packages/*/dist/`, and
`packages/*/dist-test/`. Do not hand-edit or commit them.

## Implementation rules

- Follow webpack names, option shapes, phase ordering, defaults, callback
  timing, and main observable behavior for implemented webpack surfaces.
- Reject unimplemented webpack options clearly. Do not add accepted no-op
  options or silently ignore unknown configuration.
- Expose new public API only when the underlying compilation model can support
  its observable behavior.
- Prefer Rust-native ownership, concurrency, indexed storage, and data
  structures when they preserve webpack's responsibilities and behavior.
- Keep infrastructure errors distinct from completed-compilation errors and
  preserve the asynchronous callback contracts in the JavaScript API.
- Update `CONTEXT.md` when a change introduces or resolves canonical domain
  language. Add or supersede an ADR when an architectural decision changes.

## Build and test

CI uses Node.js 24, pnpm 11.7.0, and stable Rust.

- Install dependencies: `pnpm install --frozen-lockfile`
- Build the public package: `pnpm --filter @unpack-js/core build`
- Type-check JavaScript source and tests:
  `pnpm --filter @unpack-js/core typecheck`
- Run Rust tests: `cargo test --workspace`
- Run public JavaScript package tests:
  `pnpm --filter @unpack-js/core test`
- Run benchmark-tool tests: `pnpm --filter @unpack-js/benchmarks test`
- Run the complete suite: `pnpm test`

Use targeted Rust test packages or compiled Node test files while iterating,
then run the complete relevant package suite. Run `pnpm test` before handing
off a change that affects both Rust and JavaScript boundaries. There is no
repository-wide lint script; do not invent one in validation notes.

JavaScript behavior exposed through `@unpack-js/core` should be tested through
the public package boundary. Use a webpack comparison test for important
observable behavior instead of assuming webpack semantics from memory. Follow
the local instructions in:

- `packages/unpack/test/configCases/README.md` for webpack-style config cases.
- `packages/unpack/test/e2e-cases/README.md` for emitted-bundle execution cases.

## Issue tracker and triage

Issues and PRDs are tracked in GitHub Issues for `unpack-dev/unpack`. External
pull requests are not a triage request surface. See
`docs/agents/issue-tracker.md`.

Use the five-label triage vocabulary: `needs-triage`, `needs-info`,
`ready-for-agent`, `ready-for-human`, and `wontfix`. See
`docs/agents/triage-labels.md`.

## Pull requests

Do not prefix a pull request title with `codex`, `[codex]`, `Codex:`, or any
other agent marker. Use a normal human-readable title that directly summarizes
the change. Do not add a `Validation` section to the pull request body.
