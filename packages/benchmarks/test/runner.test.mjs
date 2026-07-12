import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { isAbsolute, join } from "node:path";
import test from "node:test";
import { promisify } from "node:util";

import {
  adapters,
  applyTurbopackBuildCacheFlushPatch,
  createRspackBenchmarkConfig
} from "../src/adapters.mjs";
import {
  FIXTURE_SHAPES,
  WARM_BUILD_CHECKSUM_DELTA,
  WARM_BUILD_GRAPH_COPY
} from "../src/fixture.mjs";
import { runBenchmark, toSummaryMarkdown } from "../src/runner.mjs";

const execFileAsync = promisify(execFile);

test("summary renders loader results before a separate non-loader table", () => {
  const summary = toSummaryMarkdown({
    results: [
      summaryResult({ fixture: "large", bundler: "webpack" }),
      summaryResult({ fixture: "loader", bundler: "rspack" })
    ]
  });

  const loaderHeading = summary.indexOf("### Loader Benchmarks");
  const nonLoaderHeading = summary.indexOf("### Benchmarks Without Loaders");
  assert.ok(loaderHeading >= 0);
  assert.ok(nonLoaderHeading > loaderHeading);
  assert.equal(summary.match(/\| fixture \| bundler \|/g)?.length, 2);
  assert.match(summary.slice(loaderHeading, nonLoaderHeading), /\| loader \| rspack \|/);
  assert.doesNotMatch(summary.slice(loaderHeading, nonLoaderHeading), /\| large \|/);
  assert.match(summary.slice(nonLoaderHeading), /\| large \| webpack \|/);
  assert.doesNotMatch(summary.slice(nonLoaderHeading), /\| loader \|/);
  assert.match(
    summary,
    /watch_build_ms.*development-mode rebuild with memory cache enabled and persistent cache disabled/
  );
});

test("summary compares measurements with matching latest main results", () => {
  const current = summaryResult({ fixture: "large", bundler: "unpack" });
  const baseline = {
    ...current,
    cold_build_ms: 8,
    warm_build_ms: 2.5,
    no_cache_build_ms: 10,
    output_bytes: 200
  };
  const summary = toSummaryMarkdown(
    { results: [current] },
    { results: [baseline] }
  );

  assert.doesNotMatch(summary, /delta_vs_main/);
  assert.match(
    summary,
    /\| 10\.0 \(\+25\.0%\) \| 5\.0 \(\+100\.0%\) \| 3\.0 \(\+0\.0%\) \| 8\.0 \(-20\.0%\) \| 100 \(-50\.0%\) \| success \|/
  );
  assert.match(summary, /`\+` means slower or larger; `−` means faster or smaller/);
});

test("summary omits inline deltas when main has no matching result", () => {
  const summary = toSummaryMarkdown(
    { results: [summaryResult({ fixture: "large", bundler: "unpack" })] },
    { results: [summaryResult({ fixture: "loader", bundler: "unpack" })] }
  );

  assert.match(summary, /\| 10\.0 \| 5\.0 \| 3\.0 \| 8\.0 \| 100 \| success \|/);
  assert.doesNotMatch(summary, /\([+-][\d.]+%\)/);
});

test("runner emits persistent-cache and no-cache measurements for a verified bundle", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));
  const calls = [];

  try {
    const report = await runBenchmark({
      workspaceDir: workspace,
      fixtures: ["large"],
      bundlers: ["fake"],
      adapters: {
        fake: fakeAdapter({ calls })
      }
    });

    assert.equal(report.schema_version, 3);
    assert.equal(report.results.length, 1);
    assert.equal(report.results[0].fixture, "large");
    assert.equal(report.results[0].bundler, "fake");
    assert.equal(report.results[0].status, "success");
    assert.equal(report.results[0].cold_status, "success");
    assert.equal(report.results[0].warm_status, "success");
    assert.equal(report.results[0].watch_status, "success");
    assert.equal(report.results[0].no_cache_status, "success");
    assert.equal(report.results[0].verify_status, "success");
    assert.equal(typeof report.results[0].cold_build_ms, "number");
    assert.equal(typeof report.results[0].warm_build_ms, "number");
    assert.equal(typeof report.results[0].watch_build_ms, "number");
    assert.equal(typeof report.results[0].no_cache_build_ms, "number");
    assert.ok(report.results[0].output_bytes > 0);
    assert.deepEqual(
      calls.map(({ phase, persistentCache, cacheReadonly }) => ({
        phase,
        persistentCache,
        cacheReadonly
      })),
      [
        { phase: "cold", persistentCache: true, cacheReadonly: false },
        { phase: "watch", persistentCache: false, cacheReadonly: false },
        { phase: "warm", persistentCache: true, cacheReadonly: true },
        { phase: "no-cache", persistentCache: false, cacheReadonly: false }
      ]
    );
    assert.equal(
      calls[1].expectedChecksum,
      calls[0].expectedChecksum + WARM_BUILD_CHECKSUM_DELTA
    );
    assert.equal(calls[2].expectedChecksum, calls[1].expectedChecksum);
    assert.equal(calls[3].expectedChecksum, calls[2].expectedChecksum);
    const mutatedEntry = await readFile(
      join(workspace, "fixtures", "large", "src", "index.js"),
      "utf8"
    );
    assert.match(
      mutatedEntry,
      new RegExp(
        `// import \\* as copy${WARM_BUILD_GRAPH_COPY} from "\\./copy${WARM_BUILD_GRAPH_COPY}/Three\\.js";`
      )
    );
    assert.doesNotMatch(
      mutatedEntry,
      new RegExp(
        `^import \\* as copy${WARM_BUILD_GRAPH_COPY} from "\\./copy${WARM_BUILD_GRAPH_COPY}/Three\\.js";`,
        "m"
      )
    );
    const summary = toSummaryMarkdown(report);
    assert.match(summary, /watch_build_ms/);
    assert.match(summary, /no_cache_build_ms/);
    assert.match(summary, /\\| large \\| fake \\| fake@1\\.0\\.0 \\|/);
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("runner verifies warm builds against the mutated fixture checksum", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const report = await runBenchmark({
      workspaceDir: workspace,
      fixtures: ["large"],
      bundlers: ["fake"],
      adapters: {
        fake: fakeAdapter({ staleWarmChecksum: true })
      }
    });

    assert.equal(report.results[0].status, "warm_runtime_failed");
    assert.equal(report.results[0].cold_status, "success");
    assert.equal(report.results[0].warm_status, "runtime_failed");
    assert.equal(report.results[0].no_cache_status, "not_run");
    assert.match(report.results[0].error, /expected bundle checksum/);
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("runner marks a built bundle with the wrong checksum as runtime_failed", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const report = await runBenchmark({
      workspaceDir: workspace,
      fixtures: ["large"],
      bundlers: ["fake"],
      adapters: {
        fake: fakeAdapter({ checksumOffset: 1 })
      }
    });

    assert.equal(report.results[0].status, "runtime_failed");
    assert.equal(report.results[0].cold_status, "runtime_failed");
    assert.equal(report.results[0].warm_status, "not_run");
    assert.equal(report.results[0].no_cache_status, "not_run");
    assert.equal(report.results[0].warm_build_ms, null);
    assert.equal(report.results[0].no_cache_build_ms, null);
    assert.match(report.results[0].error, /expected bundle checksum/);
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("runner reports unsupported adapters explicitly", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const error = new Error("not available in this environment");
    error.code = "UNSUPPORTED_BUNDLER";
    const report = await runBenchmark({
      workspaceDir: workspace,
      fixtures: ["large"],
      bundlers: ["fake"],
      adapters: {
        fake: fakeAdapter({ error })
      }
    });

    assert.equal(report.results[0].status, "unsupported");
    assert.equal(report.results[0].cold_build_ms, null);
    assert.equal(report.results[0].warm_build_ms, null);
    assert.equal(report.results[0].no_cache_build_ms, null);
    assert.match(report.results[0].error, /not available/);
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("metro adapter builds a verified benchmark fixture", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const report = await runBenchmark({
      workspaceDir: workspace,
      fixtures: ["large"],
      bundlers: ["metro"],
      adapters: {
        metro: adapters.metro
      }
    });

    assert.equal(report.results[0].bundler, "metro");
    assert.equal(report.results[0].status, "success");
    assert.equal(report.results[0].cold_status, "success");
    assert.equal(report.results[0].warm_status, "success");
    assert.equal(report.results[0].no_cache_status, "success");
    assert.equal(report.results[0].verify_status, "success");
    assert.match(report.results[0].version_source, /^metro@/);
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("parcel adapter builds a verified benchmark fixture", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const report = await runBenchmark({
      workspaceDir: workspace,
      fixtures: ["large"],
      bundlers: ["parcel"],
      adapters: {
        parcel: adapters.parcel
      }
    });

    assert.equal(report.results[0].bundler, "parcel");
    assert.equal(report.results[0].status, "success");
    assert.equal(report.results[0].cold_status, "success");
    assert.equal(report.results[0].warm_status, "success");
    assert.equal(report.results[0].no_cache_status, "success");
    assert.equal(report.results[0].verify_status, "success");
    assert.match(report.results[0].version_source, /^parcel@/);
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("webpack-compatible adapters build the loader benchmark fixture", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const report = await runBenchmark({
      workspaceDir: workspace,
      fixtures: ["loader"],
      bundlers: ["unpack", "webpack", "rspack"],
      adapters: {
        unpack: adapters.unpack,
        webpack: adapters.webpack,
        rspack: adapters.rspack
      }
    });

    assert.equal(report.results.length, 3);
    for (const result of report.results) {
      assert.equal(result.fixture, "loader");
      assert.equal(result.status, "success");
      assert.equal(result.cold_status, "success");
      assert.equal(result.warm_status, "success");
      assert.equal(result.watch_status, "success");
      assert.equal(typeof result.watch_build_ms, "number");
      assert.ok(result.watch_build_ms > 0);
      assert.equal(result.no_cache_status, "success");
      assert.equal(result.verify_status, "success");
    }

    const loaderEntry = await readFile(
      join(workspace, "fixtures", "loader", "src", "index.js"),
      "utf8"
    );
    assert.match(loaderEntry, /@material-ui\/core/);
    assert.match(loaderEntry, /\.\/rome\.ts/);
    assert.doesNotMatch(loaderEntry, /loader-data|benchdata/);

    const rspackConfig = createRspackBenchmarkConfig({
      fixture: {
        context: join(workspace, "fixtures", "loader"),
        entry: "./src/index.js",
        requiresWebpackLoaders: true
      },
      outputDir: join(workspace, "output"),
      cacheDir: join(workspace, "cache")
    });
    assert.equal(rspackConfig.module.rules.length, 2);
    const [javascriptRule, typescriptRule] = rspackConfig.module.rules;
    assert.equal(javascriptRule.test.test("src/index.js"), true);
    assert.equal(javascriptRule.test.test("src/rome.ts"), false);
    assert.match(javascriptRule.loader, /swc-loader/);
    assert.equal(javascriptRule.options.jsc.parser.syntax, "ecmascript");
    assert.equal(typescriptRule.test.test("src/index.js"), false);
    assert.equal(typescriptRule.test.test("src/rome.ts"), true);
    assert.match(typescriptRule.loader, /swc-loader/);
    assert.equal(typescriptRule.options.jsc.parser.syntax, "typescript");
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("rspack benchmark config aligns with Unpack's Implemented Webpack Surface", () => {
  const config = createRspackBenchmarkConfig({
    fixture: {
      context: "/benchmark/fixture",
      entry: "./src/index.js"
    },
    outputDir: "/benchmark/output",
    cacheDir: "/benchmark/cache",
    persistentCache: true,
    cacheReadonly: true
  });

  assert.equal(config.mode, "none");
  assert.equal(config.target, "node");
  assert.deepEqual(config.externalsPresets, {
    node: false
  });
  assert.deepEqual(config.entry, {
    main: "/benchmark/fixture/src/index.js"
  });
  assert.deepEqual(config.output, {
    path: "/benchmark/output",
    filename: "main.js",
    chunkFilename: "[name].js",
    library: {
      type: "commonjs2"
    },
    clean: false,
    module: false,
    iife: false,
    chunkFormat: "commonjs",
    chunkLoading: "require",
    workerChunkLoading: false,
    wasmLoading: false,
    workerWasmLoading: false,
    asyncChunks: true,
    pathinfo: false,
    strictModuleErrorHandling: false,
    compareBeforeEmit: false
  });
  assert.equal(config.devtool, false);
  assert.deepEqual(config.resolve, {
    conditionNames: [],
    extensions: [".ts", ".tsx", ".js", ".jsx"],
    mainFields: ["main"],
    byDependency: {
      esm: {
        conditionNames: [],
        extensions: [".ts", ".tsx", ".js", ".jsx"],
        mainFields: ["main"]
      }
    }
  });
  assert.deepEqual(config.module, {
    parser: {
      javascript: {
        commonjs: false,
        commonjsMagicComments: false,
        createRequire: false,
        exportsPresence: false,
        importDynamic: true,
        importMeta: false,
        importMetaResolve: false,
        requireAlias: false,
        requireAsExpression: false,
        requireDynamic: false,
        requireResolve: false,
        url: false,
        worker: false
      }
    }
  });
  assert.equal(config.amd, false);
  assert.equal(config.node, false);
  assert.equal(config.performance, false);
  assert.deepEqual(config.experiments, {
    asyncWebAssembly: false
  });
  assert.deepEqual(config.optimization, {
    moduleIds: "named",
    chunkIds: "named",
    minimize: false,
    mergeDuplicateChunks: false,
    splitChunks: false,
    runtimeChunk: false,
    removeEmptyChunks: true,
    realContentHash: false,
    sideEffects: true,
    providedExports: true,
    concatenateModules: false,
    innerGraph: false,
    usedExports: false,
    mangleExports: false,
    inlineExports: false,
    nodeEnv: false,
    emitOnErrors: true,
    avoidEntryIife: false
  });
  assert.deepEqual(Object.keys(config.cache).sort(), [
    "readonly",
    "snapshot",
    "storage",
    "type"
  ]);
  assert.equal(config.cache.type, "persistent");
  assert.deepEqual(config.cache.storage, {
    type: "filesystem",
    directory: "/benchmark/cache"
  });
  assert.equal(config.cache.readonly, true);
  assert.deepEqual(Object.keys(config.cache.snapshot), ["managedPaths"]);
  assert.equal(config.cache.snapshot.managedPaths.length, 1);
  assert.equal(isAbsolute(config.cache.snapshot.managedPaths[0]), true);
  assert.equal(
    config.cache.snapshot.managedPaths[0].endsWith(join("@rspack", "core")),
    true
  );

  const noCacheConfig = createRspackBenchmarkConfig({
    fixture: {
      context: "/benchmark/fixture",
      entry: "./src/index.js"
    },
    outputDir: "/benchmark/output",
    cacheDir: "/benchmark/cache",
    persistentCache: false
  });
  assert.equal(noCacheConfig.cache, false);
});

test("non-loader adapters mark the loader benchmark fixture unsupported", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const report = await runBenchmark({
      workspaceDir: workspace,
      fixtures: ["loader"],
      bundlers: ["metro"],
      adapters: {
        metro: adapters.metro
      }
    });

    assert.equal(report.results[0].fixture, "loader");
    assert.equal(report.results[0].bundler, "metro");
    assert.equal(report.results[0].status, "unsupported");
    assert.equal(report.results[0].cold_build_ms, null);
    assert.match(report.results[0].error, /loader benchmark fixture/);
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("CLI accepts pnpm-style -- separator, tracing option, and writes JSON output", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));
  const outputJson = join(workspace, "results.json");

  try {
    await execFileAsync(
      process.execPath,
      [
        "src/run.mjs",
        "--",
        "--fixtures",
        "large",
        "--bundlers",
        "turbopack",
        "--workspace",
        join(workspace, "run"),
        "--output-json",
        outputJson,
        "--unpack-tracing",
        "unpack_core=trace"
      ],
      {
        cwd: join(import.meta.dirname, "..")
      }
    );

    const report = JSON.parse(await readFile(outputJson, "utf8"));
    assert.equal(report.results[0].bundler, "turbopack");
    assert.equal(report.results[0].status, "unsupported");
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("turbopack build enables persistent cache for warm measurements", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const repo = join(workspace, "next.js");
    const binary = join(repo, "target", "release", "turbopack-cli");
    const argsLog = join(repo, "turbopack-args.txt");
    const fixtureContext = join(workspace, "fixture");
    const cacheDir = join(workspace, "cache");

    await mkdir(join(repo, "target", "release"), { recursive: true });
    await mkdir(fixtureContext, { recursive: true });
    await writeFile(
      binary,
      `#!/bin/sh\nprintf '%s\\n' "$@" > "${argsLog}"\n`,
      "utf8"
    );
    await chmod(binary, 0o755);

    await adapters.turbopack.build({
      fixture: {
        context: fixtureContext,
        entry: "./src/index.js"
      },
      cacheDir,
      options: {
        turbopackRepo: repo,
        turbopackProfile: "release"
      }
    });

    const args = (await readFile(argsLog, "utf8")).trim().split("\n");
    assert.ok(args.includes("--persistent-caching"));
    assert.equal(args[args.indexOf("--cache-dir") + 1], cacheDir);
    assert.equal(args.at(-1), "./src/index.js");
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("turbopack build can use a prebuilt binary without a source checkout", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const binary = join(workspace, "tools", "turbopack-cli");
    const argsLog = join(workspace, "turbopack-args.txt");
    const fixtureContext = join(workspace, "fixture");
    const cacheDir = join(workspace, "cache");

    await mkdir(join(workspace, "tools"), { recursive: true });
    await mkdir(fixtureContext, { recursive: true });
    await writeFile(
      binary,
      `#!/bin/sh\nprintf '%s\\n' "$@" > "${argsLog}"\n`,
      "utf8"
    );
    await chmod(binary, 0o755);

    await adapters.turbopack.prepare({
      options: {
        turbopackBinary: binary
      }
    });

    await adapters.turbopack.build({
      fixture: {
        context: fixtureContext,
        entry: "./src/index.js"
      },
      cacheDir,
      options: {
        turbopackBinary: binary
      }
    });

    const args = (await readFile(argsLog, "utf8")).trim().split("\n");
    assert.ok(args.includes("--persistent-caching"));
    assert.equal(args[args.indexOf("--cache-dir") + 1], cacheDir);
    assert.equal(args.at(-1), "./src/index.js");
    assert.match(
      adapters.turbopack.versionSource({
        options: {
          turbopackBinary: binary,
          turbopackCommit: "release-source"
        }
      }),
      /^hardfist\/bundler-diff@release-source\+release-turbopack-cli$/
    );
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("turbopack build enables tracing and archives the raw trace", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const binary = join(workspace, "tools", "turbopack-cli");
    const envLog = join(workspace, "turbopack-tracing-env.txt");
    const fixtureContext = join(workspace, "fixture");
    const sourceTraceDir = join(fixtureContext, ".turbopack");
    const cacheDir = join(workspace, "cache");
    const traceDir = join(workspace, "traces");

    await mkdir(join(workspace, "tools"), { recursive: true });
    await mkdir(fixtureContext, { recursive: true });
    await writeFile(
      binary,
      `#!/bin/sh
printf '%s\\n' "$TURBOPACK_TRACING" > "${envLog}"
mkdir -p "${sourceTraceDir}"
printf 'raw trace\\n' > "${sourceTraceDir}/trace.log"
`,
      "utf8"
    );
    await chmod(binary, 0o755);

    await adapters.turbopack.build({
      fixture: {
        name: "large",
        context: fixtureContext,
        entry: "./src/index.js"
      },
      cacheDir,
      phase: "cold",
      options: {
        turbopackBinary: binary,
        turbopackTracing: "turbo-tasks",
        turbopackTracingDir: traceDir
      }
    });

    assert.equal(await readFile(envLog, "utf8"), "turbo-tasks\n");
    assert.equal(
      await readFile(join(traceDir, "large", "cold", "trace.log"), "utf8"),
      "raw trace\n"
    );
    const metadata = JSON.parse(
      await readFile(join(traceDir, "large", "cold", "metadata.json"), "utf8")
    );
    assert.equal(metadata.fixture, "large");
    assert.equal(metadata.phase, "cold");
    assert.equal(metadata.filter, "turbo-tasks");
    assert.equal(metadata.bytes, "raw trace\n".length);
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("turbopack transforms the loader fixture with swc-loader", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const binary = join(workspace, "tools", "turbopack-cli");
    const fixtureContext = join(workspace, "fixture");
    const transformedContext = join(workspace, "transformed");
    const cacheDir = join(workspace, "cache");

    await mkdir(join(workspace, "tools"), { recursive: true });
    await mkdir(join(fixtureContext, "src"), { recursive: true });
    await writeFile(binary, "#!/bin/sh\nexit 0\n", "utf8");
    await chmod(binary, 0o755);
    await writeFile(
      join(fixtureContext, "src", "index.js"),
      "const value = () => 1; export { value };\n",
      "utf8"
    );
    await writeFile(
      join(fixtureContext, "src", "rome.ts"),
      "const value: number = 1; export { value };\n",
      "utf8"
    );

    const fixture = {
        name: "loader",
        context: fixtureContext,
        entry: "./src/index.js",
        requiresWebpackLoaders: true
      };
    await adapters.turbopack.prepareBuild({
      fixture,
      outputDir: join(transformedContext, "dist"),
      phase: "cold"
    });
    await adapters.turbopack.build({
      fixture,
      outputDir: join(transformedContext, "dist"),
      cacheDir,
      phase: "cold",
      options: { turbopackBinary: binary }
    });

    const transformedJavascriptPath = join(transformedContext, "src", "index.js");
    const transformedTypescriptPath = join(transformedContext, "src", "rome.ts");
    const javascript = await readFile(transformedJavascriptPath, "utf8");
    const typescript = await readFile(transformedTypescriptPath, "utf8");
    assert.equal(javascript, "const value = ()=>1;\nexport { value };\n");
    assert.doesNotMatch(typescript, /: number/);
    assert.match(typescript, /const value = 1/);
    assert.match(await readFile(join(fixtureContext, "src", "rome.ts"), "utf8"), /: number/);

    const typescriptMtime = (await stat(transformedTypescriptPath)).mtimeMs;
    await writeFile(
      join(fixtureContext, "src", "index.js"),
      "const value = () => 2; export { value };\n",
      "utf8"
    );
    await adapters.turbopack.prepareBuild({
      fixture,
      outputDir: join(transformedContext, "dist"),
      phase: "warm"
    });
    await adapters.turbopack.build({
      fixture,
      outputDir: join(transformedContext, "dist"),
      cacheDir,
      phase: "warm",
      options: { turbopackBinary: binary }
    });

    assert.match(await readFile(transformedJavascriptPath, "utf8"), /=>2/);
    assert.equal((await stat(transformedTypescriptPath)).mtimeMs, typescriptMtime);
    assert.equal(adapters.turbopack.supportsLoaderFixture, true);
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("turbopack loader fixture reports verified cold, warm, and no-cache data", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const binary = join(workspace, "turbopack-cli");
    await writeFile(
      binary,
      `#!/usr/bin/env node
const fs = require("node:fs");
const path = require("node:path");
const args = process.argv.slice(2);
const context = args[args.indexOf("--dir") + 1];
const entry = fs.readFileSync(path.join(context, "src/index.js"), "utf8");
const checksum = entry.includes("// import * as copy${WARM_BUILD_GRAPH_COPY}")
  ? ${FIXTURE_SHAPES.loader.expectedChecksum + WARM_BUILD_CHECKSUM_DELTA}
  : ${FIXTURE_SHAPES.loader.expectedChecksum};
fs.mkdirSync(path.join(context, "dist"), { recursive: true });
fs.writeFileSync(path.join(context, "dist/index.entry.js"), "module.exports.checksum = " + checksum + ";\\n");
`,
      "utf8"
    );
    await chmod(binary, 0o755);

    const report = await runBenchmark({
      workspaceDir: join(workspace, "run"),
      fixtures: ["loader"],
      bundlers: ["turbopack"],
      adapters: { turbopack: adapters.turbopack },
      turbopackBinary: binary,
      turbopackTracing: false
    });

    assert.equal(report.results[0].status, "success");
    assert.equal(report.results[0].cold_status, "success");
    assert.equal(report.results[0].warm_status, "success");
    assert.equal(report.results[0].no_cache_status, "success");
    assert.equal(report.results[0].verify_status, "success");
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("turbopack prepare patches build shutdown to flush persistent cache", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const repo = join(workspace, "next.js");
    const buildSourcePath = join(
      repo,
      "turbopack",
      "crates",
      "turbopack-cli",
      "src",
      "build",
      "mod.rs"
    );

    await mkdir(join(repo, "turbopack", "crates", "turbopack-cli", "src", "build"), {
      recursive: true
    });
    await writeFile(
      buildSourcePath,
      `async fn build() {
    builder.build().await?;

    // Intentionally leak this \`Arc\`. Otherwise we'll waste time during process exit performing a
    // ton of drop calls.
    if !args.force_memory_cleanup {
        forget(tt);
    }
}
`,
      "utf8"
    );

    await applyTurbopackBuildCacheFlushPatch(repo);
    const patched = await readFile(buildSourcePath, "utf8");
    assert.match(patched, /if args\.common\.persistent_caching \{/);
    assert.match(patched, /tt\.stop_and_wait\(\)\.await;/);
    assert.ok(
      patched.indexOf("tt.stop_and_wait().await;") >
        patched.indexOf("builder.build().await?;")
    );

    await applyTurbopackBuildCacheFlushPatch(repo);
    assert.equal(await readFile(buildSourcePath, "utf8"), patched);
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

function summaryResult({ fixture, bundler }) {
  return {
    fixture,
    bundler,
    version_source: `${bundler}@1.0.0`,
    cold_build_ms: 10,
    warm_build_ms: 5,
    watch_build_ms: 3,
    no_cache_build_ms: 8,
    output_bytes: 100,
    status: "success"
  };
}

function fakeAdapter({ checksumOffset = 0, error, calls, staleWarmChecksum = false } = {}) {
  let coldChecksum;
  return {
    name: "fake",
    versionSource: () => "fake@1.0.0",
    async watchBuild({ fixture, outputDir, mutateAfterInitialBuild }) {
      const entryFile = join(outputDir, "main.js");
      await writeFile(entryFile, `exports.checksum = ${fixture.expectedChecksum};\n`, "utf8");
      await mutateAfterInitialBuild();
      calls?.push({
        phase: "watch",
        persistentCache: false,
        cacheReadonly: false,
        expectedChecksum: fixture.expectedChecksum
      });
      await writeFile(entryFile, `exports.checksum = ${fixture.expectedChecksum};\n`, "utf8");
      return { entryFile, rebuildMs: 1 };
    },
    async build({ fixture, outputDir, phase, persistentCache, cacheReadonly }) {
      calls?.push({
        phase,
        persistentCache,
        cacheReadonly,
        expectedChecksum: fixture.expectedChecksum
      });
      if (error) {
        throw error;
      }
      if (phase === "cold") {
        coldChecksum = fixture.expectedChecksum;
      }
      const checksum =
        staleWarmChecksum && phase === "warm"
          ? coldChecksum
          : fixture.expectedChecksum;
      const entryFile = join(outputDir, "main.js");
      await writeFile(
        entryFile,
        `exports.checksum = ${checksum + checksumOffset};\n`,
        "utf8"
      );
      return { entryFile };
    }
  };
}
