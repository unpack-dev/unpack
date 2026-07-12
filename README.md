# Unpack

[![CI](https://github.com/unpack-dev/unpack/actions/workflows/ci.yml/badge.svg)](https://github.com/unpack-dev/unpack/actions/workflows/ci.yml)

Unpack is an experimental JavaScript bundler written in Rust. It explores how
far bundler performance can be pushed while staying closely aligned with
webpack's architecture, terminology, configuration, and observable behavior.

> [!IMPORTANT]
> Unpack is under active development. `@unpack-js/core` currently implements a
> focused subset of webpack's API and is not yet a drop-in webpack replacement.

## What is implemented

- An ESM-first Node.js API exposed as `@unpack-js/core`.
- Webpack-shaped compiler lifecycle methods: `run`, `watch`, and `close`.
- Static ESM dependencies, dynamic imports, code splitting, and webpack-shaped
  CommonJS bundle output.
- JavaScript, JSON, and asset module types, plus a minimal JavaScript loader
  pipeline.
- Source maps, multiple named entries, build statistics, and selected compiler
  and compilation hooks.
- Memory and persistent filesystem build caches with configurable snapshot
  validation.
- Export analysis and side-effects optimization on the currently supported
  JavaScript surface.
- Comparison tests against a pinned webpack version and diagnostic
  cross-bundler benchmarks.

Implemented webpack-shaped surfaces are expected to match webpack's observable
behavior. Unsupported options should fail clearly instead of being silently
accepted. See the
[implementation differences](docs/implementation/webpack-implementation-differences.md)
for the current boundaries.

## Repository setup

The CI environment uses Node.js 24, pnpm 11.7.0, and the stable Rust toolchain.
Install those tools, including Cargo and a native C/C++ build toolchain, then
run:

```sh
corepack enable
pnpm install --frozen-lockfile
pnpm --filter @unpack-js/core build
```

The package build compiles the TypeScript wrapper, builds the `unpack_node`
native addon, and copies the addon into `packages/unpack/dist`.

To build the native addon with Cargo's release profile:

```sh
UNPACK_NATIVE_PROFILE=release pnpm --filter @unpack-js/core build
```

## JavaScript API

After building the workspace package, the default export creates a compiler
from webpack-shaped options:

```js
import { resolve } from "node:path";
import unpack from "@unpack-js/core";

const compiler = unpack({
  context: process.cwd(),
  mode: "development",
  entry: "./src/index.js",
  output: {
    path: resolve("dist")
  }
});

const stats = await new Promise((resolve, reject) => {
  compiler.run((error, result) => {
    if (error) reject(error);
    else resolve(result);
  });
});

if (stats.hasErrors()) {
  console.error(stats.toJson().errors);
} else {
  console.log(stats.toJson().assets);
}

await new Promise((resolve, reject) => {
  compiler.close((error) => {
    if (error) reject(error);
    else resolve();
  });
});
```

The emitted entry asset is `main.js` for a string entry. Named entry objects
emit one entry asset per key. The public types in
[`packages/unpack/src/index.ts`](packages/unpack/src/index.ts) are the source of
truth for currently accepted options and exposed lifecycle objects.

## Development commands

| Command | Purpose |
| --- | --- |
| `pnpm test` | Run the complete Rust and JavaScript test suite. |
| `cargo test --workspace` | Run all Rust workspace tests. |
| `pnpm --filter @unpack-js/core test` | Build and test the public JavaScript package. |
| `pnpm --filter @unpack-js/core typecheck` | Type-check package source and tests. |
| `pnpm --filter @unpack-js/benchmarks test` | Test the benchmark tooling. |
| `pnpm benchmark:bundlers -- --bundlers unpack,webpack` | Compare selected bundlers on the shared fixtures. |

Cross-bundler benchmark results are diagnostic signals, not compatibility
claims or merge gates. The benchmark runner writes temporary data to
`.benchmark-work/`.

## Architecture

```text
packages/unpack/       Public ESM TypeScript API and JavaScript tests
        │
        ▼
crates/unpack_node/    Internal N-API bridge built with napi-rs
        │
        ▼
crates/unpack_core/    Rust compiler, graphs, caching, and code generation

packages/benchmarks/   Cross-bundler benchmark fixtures and runner
docs/adr/              Architecture decision records
docs/implementation/   Implementation notes and alignment boundaries
```

The TypeScript layer owns public option normalization and lifecycle behavior;
the N-API layer is an internal interop boundary; and the Rust core owns bundler
state and compilation work. This separation is intentional and documented in
[ADR 0048](docs/adr/0048-split-rust-core-node-binding-and-javascript-package.md).

## Contributing

Before changing behavior, read [`CONTEXT.md`](CONTEXT.md) for the project's
domain language and the relevant records in [`docs/adr/`](docs/adr/). Changes
to a webpack-shaped public surface should include JavaScript API tests and,
when the observable behavior matters, a comparison with the pinned webpack
reference.

Useful test layouts:

- [`packages/unpack/test/configCases/`](packages/unpack/test/configCases/README.md)
  mirrors webpack's config-case organization.
- [`packages/unpack/test/e2e-cases/`](packages/unpack/test/e2e-cases/README.md)
  exercises emitted bundles in isolated fixtures.

Issues and project work are tracked in
[GitHub Issues](https://github.com/unpack-dev/unpack/issues).
