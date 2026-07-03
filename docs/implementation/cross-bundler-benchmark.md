# Cross-Bundler Benchmark

The Cross-Bundler Benchmark compares Unpack with webpack, Rspack, Rolldown, and Turbopack on generated Benchmark Fixtures. The results are diagnostic signals for maintainers; they are not merge gates and they are not compatibility claims.

## Local Run

Build the Unpack JavaScript API and run the benchmark:

```sh
pnpm benchmark:bundlers -- --workspace .benchmark-work/local
```

To run only the fast local smoke subset:

```sh
pnpm --filter @unpack-js/benchmarks bench -- --fixtures small --bundlers unpack,webpack,rspack,rolldown
```

The cross-bundler benchmark prints Unpack internal tracing details to stderr for
each Unpack build phase. The default filter is
`unpack_core=trace,unpack_node=trace`, which shows the coarse compiler spans and
their close-time busy/idle durations. Pass `--no-unpack-tracing` for quiet
benchmark logs, or `--unpack-tracing <filter>` to use a custom Rust
`tracing-subscriber` filter.

Turbopack requires a fixed Next.js checkout:

```sh
git init .benchmark-tools/next.js
git -C .benchmark-tools/next.js remote add origin https://github.com/vercel/next.js.git
git -C .benchmark-tools/next.js fetch --depth=1 --filter=blob:none origin a88f25caf0070b582a8ed83b1ae9e7135d7fd3bc
git -C .benchmark-tools/next.js checkout --detach FETCH_HEAD

pnpm --filter @unpack-js/benchmarks bench -- \
  --turbopack-repo .benchmark-tools/next.js \
  --turbopack-commit a88f25caf0070b582a8ed83b1ae9e7135d7fd3bc
```

During `prepare`, the Turbopack adapter applies a benchmark-local patch to the
fixed checkout so `turbopack-cli build` explicitly stops TurboTasks before
process exit. Turbopack build sessions use shutdown-time persistent cache
storage, so this flushes the cold build cache for the warm measurement.

## Result Shape

The runner emits a Markdown summary and can write the raw JSON report with `--output-json`.

Important fields:

- `cold_build_ms`: build time after clearing benchmark-owned output and persistent cache state.
- `warm_build_ms`: build time after a cold build in the same job, preserving benchmark-owned persistent cache state, modifying one generated fixture module, and verifying the updated bundle checksum.
- `no_cache_build_ms`: build time for an additional clean build with persistent cache disabled. Bundlers without a persistent-cache option run this as a separate clean one-shot build.
- `output_bytes`: bytes emitted under the benchmark output path, excluding runner metadata.
- `version_source`: the npm package version or fixed source commit used for the bundler.
- `status`: `success`, `unsupported`, `setup_failed`, `build_failed`, `runtime_failed`, or a warm/no-cache build variant.

Runtime verification is separate from build timing. A Bundle that builds but does not export the expected checksum is marked `runtime_failed` and should not be treated as a valid performance result.

## CI

The `Cross-Bundler Benchmarks` workflow runs on pushes to `main`, pull requests, and manual dispatch. It writes the table to the GitHub Actions job summary, creates or updates a benchmark summary comment on pull requests, writes an Unpack and webpack phase timing summary for the `large` fixture to a new pull request issue comment, and uploads the JSON report, Markdown summary, timing summary, and raw timing log as artifacts.

The workflow builds the Unpack native addon with `UNPACK_NATIVE_PROFILE=release` so benchmark results compare optimized native builds.

The workflow is intentionally non-blocking. Benchmark setup failures, external toolchain failures, or runtime verification failures should be visible in the workflow output without blocking unrelated code from merging.
