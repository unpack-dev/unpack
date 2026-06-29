import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { promisify } from "node:util";

import { runBenchmark, toSummaryMarkdown } from "../src/runner.mjs";

const execFileAsync = promisify(execFile);

test("runner emits cold and warm measurements for a verified bundle", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const report = await runBenchmark({
      workspaceDir: workspace,
      fixtures: ["small"],
      bundlers: ["fake"],
      adapters: {
        fake: fakeAdapter()
      }
    });

    assert.equal(report.schema_version, 1);
    assert.equal(report.results.length, 1);
    assert.equal(report.results[0].fixture, "small");
    assert.equal(report.results[0].bundler, "fake");
    assert.equal(report.results[0].status, "success");
    assert.equal(report.results[0].cold_status, "success");
    assert.equal(report.results[0].warm_status, "success");
    assert.equal(report.results[0].verify_status, "success");
    assert.equal(typeof report.results[0].cold_build_ms, "number");
    assert.equal(typeof report.results[0].warm_build_ms, "number");
    assert.ok(report.results[0].output_bytes > 0);
    assert.match(toSummaryMarkdown(report), /\\| small \\| fake \\| fake@1\\.0\\.0 \\|/);
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
    assert.equal(report.results[0].warm_build_ms, null);
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

function fakeAdapter({ checksumOffset = 0, error } = {}) {
  return {
    name: "fake",
    versionSource: () => "fake@1.0.0",
    async build({ fixture, outputDir }) {
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
