# Cross-Bundler Benchmark

The Cross-Bundler Benchmark compares Unpack with webpack, Rspack, Rolldown, Metro, Parcel, and Turbopack on the `large` and `loader` Benchmark Fixtures. The `large` fixture is derived from webpack's `benchmark/cases/all` workload and generated locally so benchmark runs do not need network access. The `loader` fixture uses the same `large` workload with an added webpack-compatible loader pipeline; adapters without webpack loader support report `unsupported`. The results are diagnostic signals for maintainers; they are not merge gates and they are not compatibility claims.

## Local Run

Build the Unpack JavaScript API and run the benchmark:

```sh
pnpm benchmark:bundlers -- --workspace .benchmark-work/local
```

To run a local subset of bundlers:

```sh
pnpm --filter @unpack-js/benchmarks bench -- --fixtures large,loader --bundlers unpack,webpack,rspack,rolldown,metro,parcel
```

The cross-bundler benchmark prints Unpack internal tracing details to stderr for
each Unpack build phase. The default filter is
`unpack_core=trace,unpack_node=trace`, which shows the coarse compiler spans and
their close-time busy/idle durations. Pass `--no-unpack-tracing` for quiet
benchmark logs, or `--unpack-tracing <filter>` to use a custom Rust
`tracing-subscriber` filter.

Turbopack can run against the prebuilt `turbopack-cli` release produced by
`hardfist/bundler-diff`, or against a fixed Next.js checkout for local
source-level comparisons. Pass `--turbopack-tracing <filter>` to set
`TURBOPACK_TRACING` for `turbopack-cli`; `turbo-tasks` captures detailed
TurboTasks execution traces. When `--turbopack-tracing-dir <path>` is set, the
runner copies each raw `.turbopack/trace.log` file into that directory by
fixture and build phase.

```sh
git init .benchmark-tools/next.js
git -C .benchmark-tools/next.js remote add origin https://github.com/vercel/next.js.git
git -C .benchmark-tools/next.js fetch --depth=1 --filter=blob:none origin a88f25caf0070b582a8ed83b1ae9e7135d7fd3bc
git -C .benchmark-tools/next.js checkout --detach FETCH_HEAD

pnpm --filter @unpack-js/benchmarks bench -- \
  --turbopack-repo .benchmark-tools/next.js \
  --turbopack-commit a88f25caf0070b582a8ed83b1ae9e7135d7fd3bc \
  --turbopack-tracing turbo-tasks \
  --turbopack-tracing-dir .benchmark-work/local/turbopack-traces
```

During `prepare`, the Turbopack adapter applies a benchmark-local patch to the
fixed checkout so `turbopack-cli build` explicitly stops TurboTasks before
process exit. Turbopack build sessions use shutdown-time persistent cache
storage, so this flushes the cold build cache for the warm measurement.

To create a shareable trace index after a run:

```sh
node packages/benchmarks/src/turbopack-trace-index.mjs \
  .benchmark-work/local/turbopack-traces \
  .benchmark-work/local/turbopack-traces.md \
  --html .benchmark-work/local/turbopack-traces/index.html
```

The raw traces can be viewed with the Turbopack trace viewer. Start a local
trace server with `pnpm next internal trace <trace.log>` or
`cargo run --bin turbo-trace-server --release -- <trace.log>`, then open
<https://trace.nextjs.org/>.

## Result Shape

The runner emits a Markdown summary and can write the raw JSON report with `--output-json`.

Important fields:

- `cold_build_ms`: build time after clearing benchmark-owned output and persistent cache state.
- `warm_build_ms`: build time after a cold build in the same job, preserving benchmark-owned persistent cache state, rewriting the fixture entry to comment out one generated module import/export, and verifying the updated bundle checksum.
- `no_cache_build_ms`: build time for an additional clean build with persistent cache disabled. Bundlers without a persistent-cache option run this as a separate clean one-shot build.
- `output_bytes`: bytes emitted under the benchmark output path, excluding runner metadata.
- `version_source`: the npm package version or fixed source commit used for the bundler.
- `status`: `success`, `unsupported`, `setup_failed`, `build_failed`, `runtime_failed`, or a warm/no-cache build variant.

Runtime verification is separate from build timing. A Bundle that builds but does not export the expected checksum is marked `runtime_failed` and should not be treated as a valid performance result.

## CI

The `Cross-Bundler Benchmarks` workflow runs on pushes to `main`, pull requests, and manual dispatch. CI runs compare Unpack with webpack, Rspack, Rolldown, Metro, Parcel, and Turbopack across the `large` and `loader` fixtures by default. The workflow downloads the `turbopack-cli-main` release artifact from `hardfist/bundler-diff` and passes it to the benchmark runner with `--turbopack-binary`; manual dispatch can set `include_turbopack` to `false` when a non-Turbopack run is needed. Turbopack CI runs enable `TURBOPACK_TRACING=turbo-tasks`, copy raw trace files into `.benchmark-work/ci/turbopack-traces`, generate a Markdown and HTML trace index, and include that index in the GitHub Actions job summary and pull request timing comment. The workflow writes the benchmark table to the GitHub Actions job summary, creates or updates a benchmark summary comment on pull requests, writes an Unpack and webpack phase timing summary for the `large` fixture to a new pull request issue comment, and uploads the JSON report, Markdown summary, timing summary, raw timing log, Turbopack traces, and trace index artifacts.

The workflow builds the Unpack native addon with `UNPACK_NATIVE_PROFILE=release` so benchmark results compare optimized native builds.

The workflow is intentionally non-blocking. Benchmark setup failures, external toolchain failures, or runtime verification failures should be visible in the workflow output without blocking unrelated code from merging.
