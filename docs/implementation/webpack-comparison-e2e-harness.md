# Webpack comparison e2e harness

This note resolves
[Define the non-lifecycle webpack comparison harness](https://github.com/unpack-dev/unpack/issues/96)
for the wayfinder map
[Add webpack comparison e2e coverage for implemented surfaces](https://github.com/unpack-dev/unpack/issues/94).

## Decision

Add a small shared comparison helper module when the first non-lifecycle
Webpack Comparison Test lands, and keep scenario assertions in feature-specific
test files. The helper should hide filesystem setup, webpack/Unpack execution,
bundle execution, asset reads, and cleanup; it should not hide the observable
behavior each scenario asserts.

## File organization

Use this layout for the first expansion:

- `packages/unpack/test/webpack-lifecycle-alignment.test.ts` stays dedicated to
  lifecycle comparison rows.
- `packages/unpack/test/webpack-comparison-helpers.ts` holds generic comparison
  helpers. This file is compiled by `tsc -p tsconfig.test.json` but is not run
  by `node --test dist-test/test/*.test.js` because it does not end in
  `.test.ts`.
- Feature tests live beside the existing tests, named by behavior area:
  `webpack-output-alignment.test.ts`, `webpack-cache-snapshot-alignment.test.ts`,
  or similarly narrow names chosen by the strategy tickets.
- Ordinary Unpack-only regression tests remain in `api.test.ts` unless a test
  executes both webpack and Unpack.

Do not create one broad non-lifecycle comparison file. The inventory already
shows multiple independent behavior areas; feature files keep failures readable
and let later strategy tickets graduate implementation tickets without touching
unrelated scenarios.

## Helper shape

The helper should expose boring primitives rather than a test DSL:

```ts
interface FixtureFiles {
  [path: string]: string;
}

interface ComparisonFixture {
  webpackRoot: string;
  unpackRoot: string;
  cleanup(): Promise<void>;
}

interface BuildObservation {
  err: Error | null;
  hasStats: boolean;
  hasErrors: boolean | undefined;
  assets: string[];
  outputPath: string | undefined;
}
```

Recommended helper functions:

- `createComparisonFixture(prefix, files)`: creates isolated webpack and Unpack
  fixture roots with the same file tree.
- `webpackNodeOptions(root, overrides?)`: returns a pinned-webpack
  `Configuration` with `context: root`, `mode: "none"`, `target: "node"`,
  `entry: "./src/index.js"`, `output.path: join(root, "dist")`, and
  `output.library.type: "commonjs2"` when the scenario needs
  `require("./main.js")` to expose entry exports.
- `unpackOptions(root, overrides?)`: returns matching `@unpack-js/core` options
  with `context: root`, `mode: "none"`, `entry: "./src/index.js"`, and
  `output.path: join(root, "dist")`.
- `runWebpack(options)` and `runUnpack(options)`: run one build and return a
  normalized `BuildObservation`. They should preserve the original `err` and
  `stats` for tests that need deeper inspection.
- `readAsset(root, name)`: reads an emitted asset from the fixture `dist`.
- `listAssets(stats)`: normalizes asset names from webpack and Unpack stats.
- `runNodeScript(root, script)`: executes generated bundles from `root/dist`
  with `process.execPath` and returns stdout, stderr, status, and any thrown
  process error.
- `captureSynchronousThrow(callback)` and `delay(ms)`: keep the lifecycle helper
  behavior available if future comparison files need validation or timing
  observations outside the lifecycle file.

Keep the helper return objects plain. Tests should still spell out assertions
like `assert.deepEqual(unpackRuntime.stdout, webpackRuntime.stdout)` or
`assert.match(await readAsset(unpackRoot, "main.js"), /__webpack_require__/)`
in the test body.

## Fixture conventions

Use two fixture roots per comparison, one for webpack and one for Unpack. Do not
run both bundlers in the same root because cache state, output paths, generated
chunks, and package manager metadata can interfere with each other.

Fixture file paths should be POSIX-style relative paths such as
`src/index.js`. The helper should create parent directories, write UTF-8 text,
and return absolute roots under `mkdtemp(join(tmpdir(), prefix))`.

Default scenarios should use:

- `mode: "none"` to avoid production optimizations obscuring behavior.
- `target: "node"` for webpack because Unpack's first runtime target is fixed
  to Node/CommonJS require chunk loading.
- `output.path: join(root, "dist")` for both bundlers.
- `output.library.type: "commonjs2"` for webpack when the test requires the
  bundle and compares entry exports, because Unpack's generated initial asset
  exports entry exports with `module.exports`.

Scenario tests may override entry, mode, cache, snapshot, or output settings,
but the override should be local and visible in the test body.

## Generated bundle execution

Bundle execution tests should run Node in the emitted output directory:

```ts
const webpackRuntime = await runNodeScript(webpackRoot, `
  Promise.resolve(require("./main.js").load())
    .then((value) => console.log(JSON.stringify(value)));
`);
const unpackRuntime = await runNodeScript(unpackRoot, `
  Promise.resolve(require("./main.js").load())
    .then((value) => console.log(JSON.stringify(value)));
`);

assert.equal(unpackRuntime.status, 0);
assert.equal(webpackRuntime.status, 0);
assert.equal(unpackRuntime.stdout.trim(), webpackRuntime.stdout.trim());
```

Use `process.execPath`, not a bare `node` command, so tests use the same Node
binary that runs the test process. Capture stdout and stderr in the helper, but
keep assertions in the scenario.

## Asset inspection

Asset inspection tests should normalize only the data they actually compare:

- Asset names: sort names before comparison.
- Asset text: use structural regex checks for webpack-shaped runtime helpers
  and emitted module identifiers; do not compare whole files.
- Source maps: compare presence, asset names, and essential structure only.
- Errors: compare error taxonomy and observable `Stats` state; exact message
  text remains Unpack-only unless a strategy ticket explicitly chooses a stable
  wording assertion.

The helper should not provide snapshot-style full asset assertions.

## Recording alignment gaps

When webpack and current Unpack behavior differ but the suite must stay green,
write an observation-style test:

- Assert the webpack observation under a `webpack...` variable.
- Assert the current Unpack observation under an `unpack...` variable.
- Name the test with `observes ...` rather than `aligns ...`.
- Add or update the relevant matrix or implementation document to classify the
  difference as an alignment gap or documented deviation.
- Convert the scenario to shared assertions only after Unpack behavior changes.

Shared alignment tests should use a single normalized assertion shape, for
example:

```ts
assert.deepEqual(normalizeUnpack(unpackObservation), normalizeWebpack(webpackObservation));
```

Observation-style tests should not hide differences behind a helper that returns
`pass: true`. The differing facts need to remain visible in the test body and in
the linked strategy document.

## Cleanup conventions

Tests should use `try/finally` and `rm(root, { recursive: true, force: true })`
for both fixture roots. Helpers may expose one `cleanup()` method, but the test
body should still call it from `finally` so leaked temporary directories are not
masked by assertion failures.

Compilers created without callback entry should be closed explicitly. Webpack
compilers should use `compiler.close(callback)` after `compiler.run`; Unpack
compilers should use `compiler.close(callback)` unless the callback-entry API
itself is the behavior under test.

Watch tests should close `Watching` before closing the compiler. If a test uses
polling or timers, it should avoid shared roots and use explicit timeouts only
where the observable behavior requires them.

## Non-goals

- Do not introduce a new test framework.
- Do not import webpack's test suite.
- Do not compare complete generated files or exact source map contents.
- Do not make helper abstractions that name product behavior, such as
  `assertWebpackCompatibleBundle()`. The behavior belongs in each test.
- Do not move lifecycle tests into the non-lifecycle files; lifecycle
  comparison rows already have a dedicated file.

## Follow-up implications

The next strategy tickets can assume this harness shape and should decide only
which scenarios to graduate:

- [Decide lifecycle comparison expansion scope](https://github.com/unpack-dev/unpack/issues/97)
  should keep using `webpack-lifecycle-alignment.test.ts`, though generic
  helpers may be shared if they remain behavior-neutral.
- [Decide output and runtime semantic comparison scope](https://github.com/unpack-dev/unpack/issues/98)
  should create output/runtime scenarios around bundle execution and structural
  asset checks.
- [Decide watch cache snapshot and logging comparison scope](https://github.com/unpack-dev/unpack/issues/99)
  should use the same fixture/run primitives, but avoid comparing
  Unpack-private persistent cache files.
