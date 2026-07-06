import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { promisify } from "node:util";

import { adapters, applyTurbopackBuildCacheFlushPatch } from "../src/adapters.mjs";
import {
  WARM_BUILD_CHECKSUM_DELTA,
  WARM_BUILD_GRAPH_COPY
} from "../src/fixture.mjs";
import { runBenchmark, toSummaryMarkdown } from "../src/runner.mjs";

const execFileAsync = promisify(execFile);

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

    assert.equal(report.schema_version, 2);
    assert.equal(report.results.length, 1);
    assert.equal(report.results[0].fixture, "large");
    assert.equal(report.results[0].bundler, "fake");
    assert.equal(report.results[0].status, "success");
    assert.equal(report.results[0].cold_status, "success");
    assert.equal(report.results[0].warm_status, "success");
    assert.equal(report.results[0].no_cache_status, "success");
    assert.equal(report.results[0].verify_status, "success");
    assert.equal(typeof report.results[0].cold_build_ms, "number");
    assert.equal(typeof report.results[0].warm_build_ms, "number");
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
        { phase: "warm", persistentCache: true, cacheReadonly: true },
        { phase: "no-cache", persistentCache: false, cacheReadonly: false }
      ]
    );
    assert.equal(
      calls[1].expectedChecksum,
      calls[0].expectedChecksum + WARM_BUILD_CHECKSUM_DELTA
    );
    assert.equal(calls[2].expectedChecksum, calls[1].expectedChecksum);
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
      bundlers: ["webpack", "rspack"],
      adapters: {
        webpack: adapters.webpack,
        rspack: adapters.rspack
      }
    });

    assert.equal(report.results.length, 2);
    for (const result of report.results) {
      assert.equal(result.fixture, "loader");
      assert.equal(result.status, "success");
      assert.equal(result.cold_status, "success");
      assert.equal(result.warm_status, "success");
      assert.equal(result.no_cache_status, "success");
      assert.equal(result.verify_status, "success");
    }

    const loaderEntry = await readFile(
      join(workspace, "fixtures", "loader", "src", "index.js"),
      "utf8"
    );
    assert.match(loaderEntry, /@material-ui\/core/);
    assert.match(loaderEntry, /\.\/rome\.ts/);
    assert.match(loaderEntry, /\.\/loader-data\/item0\.benchdata/);
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
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
    assert.match(report.results[0].error, /webpack loader benchmark fixture/);
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

function fakeAdapter({ checksumOffset = 0, error, calls, staleWarmChecksum = false } = {}) {
  let coldChecksum;
  return {
    name: "fake",
    versionSource: () => "fake@1.0.0",
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
