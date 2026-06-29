import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { promisify } from "node:util";

import { adapters, applyTurbopackBuildCacheFlushPatch } from "../src/adapters.mjs";
import { runBenchmark, toSummaryMarkdown } from "../src/runner.mjs";

const execFileAsync = promisify(execFile);

test("runner emits persistent-cache and no-cache measurements for a verified bundle", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));
  const calls = [];

  try {
    const report = await runBenchmark({
      workspaceDir: workspace,
      fixtures: ["small"],
      bundlers: ["fake"],
      adapters: {
        fake: fakeAdapter({ calls })
      }
    });

    assert.equal(report.schema_version, 2);
    assert.equal(report.results.length, 1);
    assert.equal(report.results[0].fixture, "small");
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
      calls.map(({ phase, persistentCache }) => ({ phase, persistentCache })),
      [
        { phase: "cold", persistentCache: true },
        { phase: "warm", persistentCache: true },
        { phase: "no-cache", persistentCache: false }
      ]
    );
    const summary = toSummaryMarkdown(report);
    assert.match(summary, /no_cache_build_ms/);
    assert.match(summary, /\\| small \\| fake \\| fake@1\\.0\\.0 \\|/);
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("runner marks a built bundle with the wrong checksum as runtime_failed", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const report = await runBenchmark({
      workspaceDir: workspace,
      fixtures: ["small"],
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
      fixtures: ["small"],
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

test("CLI accepts pnpm-style -- separator and writes JSON output", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));
  const outputJson = join(workspace, "results.json");

  try {
    await execFileAsync(
      process.execPath,
      [
        "src/run.mjs",
        "--",
        "--fixtures",
        "small",
        "--bundlers",
        "turbopack",
        "--workspace",
        join(workspace, "run"),
        "--output-json",
        outputJson
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

function fakeAdapter({ checksumOffset = 0, error, calls } = {}) {
  return {
    name: "fake",
    versionSource: () => "fake@1.0.0",
    async build({ fixture, outputDir, phase, persistentCache }) {
      calls?.push({ phase, persistentCache });
      if (error) {
        throw error;
      }
      const entryFile = join(outputDir, "main.js");
      await writeFile(
        entryFile,
        `exports.checksum = ${fixture.expectedChecksum + checksumOffset};\n`,
        "utf8"
      );
      return { entryFile };
    }
  };
}
