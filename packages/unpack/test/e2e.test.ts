import { execFile } from "node:child_process";
import { cp, mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import test from "node:test";
import assert from "node:assert/strict";

import unpack from "@unpack-js/core";
import type { Stats, UnpackOptions } from "@unpack-js/core";

const execFileAsync = promisify(execFile);
const casesRoot = join(
  dirname(fileURLToPath(import.meta.url)),
  "..",
  "..",
  "test",
  "e2e-cases"
);
const defaultEntry = "./src/index.js";
const defaultEntryAsset = "main.js";
const defaultCompilerOptions = {
  sourcemap: false
} satisfies Pick<UnpackOptions, "sourcemap">;

interface BundleExecutionCase {
  id: string;
  path: string;
  entry?: UnpackOptions["entry"];
  entryAsset?: string;
  runtimeExpression: string;
  expected: unknown;
  expectedErrors?: string[];
  expectedErrorCount?: number;
  expectedAssets?: string[];
}

interface BundleExecutionCaseManifest {
  entry?: UnpackOptions["entry"];
  entryAsset?: string;
  runtimeExpression: string;
  expected: unknown;
  expectedErrors?: string[];
  expectedErrorCount?: number;
  expectedAssets?: string[];
}

const bundleExecutionCases = await readBundleExecutionCases();

for (const bundleCase of bundleExecutionCases) {
  test(`emitted bundle ${bundleCase.id}`, async () => {
    await runBundleExecutionCase(bundleCase);
  });
}

async function runBundleExecutionCase(bundleCase: BundleExecutionCase) {
  const fixture = await createFixture(bundleCase.path);
  const outputPath = join(fixture, "dist");

  try {
    const { err, stats } = await runCompiler({
      ...defaultCompilerOptions,
      context: fixture,
      entry: bundleCase.entry ?? defaultEntry,
      output: { path: outputPath }
    });

    assert.equal(err, null);
    const errors = stats?.toJson().errors ?? [];
    if (bundleCase.expectedErrors === undefined && bundleCase.expectedErrorCount === undefined) {
      assert.equal(stats?.hasErrors(), false);
    } else {
      if (bundleCase.expectedErrors !== undefined) {
        assert.equal(stats?.hasErrors(), true);
        for (const expectedError of bundleCase.expectedErrors) {
          assert.ok(
            errors.some((error) => error.message.includes(expectedError)),
            `expected Stats error containing ${JSON.stringify(expectedError)}`
          );
        }
      }
      if (bundleCase.expectedErrorCount !== undefined) {
        assert.equal(errors.length, bundleCase.expectedErrorCount);
      }
    }
    assert.ok(
      stats?.toJson().assets.some((asset) => asset.name === (bundleCase.entryAsset ?? defaultEntryAsset))
    );
    assert.ok(
      (await readdir(outputPath)).includes(bundleCase.entryAsset ?? defaultEntryAsset)
    );
    if (bundleCase.expectedAssets !== undefined) {
      assert.deepEqual(
        stats?.toJson().assets.map((asset) => asset.name).sort(),
        [...bundleCase.expectedAssets].sort()
      );
    }

    const result = await executeEntryExpression(
      outputPath,
      bundleCase.entryAsset ?? defaultEntryAsset,
      bundleCase.runtimeExpression
    );

    assert.equal(result, JSON.stringify(bundleCase.expected));
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
}

async function readBundleExecutionCases() {
  const entries = await readdir(casesRoot, { withFileTypes: true });
  const caseDirectories = entries
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();

  assert.notEqual(caseDirectories.length, 0, "expected at least one e2e case");

  return Promise.all(
    caseDirectories.map(async (id): Promise<BundleExecutionCase> => {
      const casePath = join(casesRoot, id);
      const manifest = parseCaseManifest(
        id,
        await readFile(join(casePath, "case.json"), "utf8")
      );

      return {
        id,
        path: casePath,
        ...manifest
      };
    })
  );
}

async function createFixture(casePath: string) {
  const fixturePath = await mkdtemp(join(tmpdir(), "unpack-e2e-"));
  const entries = await readdir(casePath, { withFileTypes: true });

  await Promise.all(
    entries
      .filter((entry) => entry.name !== "case.json")
      .map((entry) =>
        cp(join(casePath, entry.name), join(fixturePath, entry.name), {
          recursive: true
        })
      )
  );

  return fixturePath;
}

async function runCompiler(options: UnpackOptions) {
  return new Promise<{ err: Error | null; stats?: Stats }>((resolve) => {
    unpack(options, (err, stats) => {
      resolve({ err, stats });
    });
  });
}

async function executeEntryExpression(
  outputPath: string,
  entryAsset: string,
  runtimeExpression: string
) {
  try {
    const { stdout } = await execFileAsync(
      process.execPath,
      ["-e", createNodeScript(entryAsset, runtimeExpression)],
      {
        cwd: outputPath,
        encoding: "utf8"
      }
    );
    return stdout.trim();
  } catch (error) {
    const result = error as { stdout?: string; stderr?: string };
    assert.fail(
      `node failed\nstdout:\n${result.stdout ?? ""}\nstderr:\n${result.stderr ?? ""}`
    );
  }
}

function createNodeScript(entryAsset: string, runtimeExpression: string) {
  return `
    const entry = require(${JSON.stringify(`./${entryAsset}`)});

    Promise.resolve(${runtimeExpression})
      .then((value) => {
        console.log(JSON.stringify(value));
      })
      .catch((error) => {
        console.error(error && error.stack || error);
        process.exit(1);
      });
  `;
}

function parseCaseManifest(id: string, source: string): BundleExecutionCaseManifest {
  const parsed = JSON.parse(source) as unknown;

  assert.ok(isRecord(parsed), `${id}/case.json must define an object`);

  const manifest = parsed as Partial<BundleExecutionCaseManifest>;

  assert.equal(
    typeof manifest.runtimeExpression,
    "string",
    `${id}/case.json must define runtimeExpression`
  );
  assert.ok(
    Object.hasOwn(manifest, "expected"),
    `${id}/case.json must define expected`
  );

  if (manifest.entry !== undefined) {
    assert.ok(
      isEntryOption(manifest.entry),
      `${id}/case.json entry must match UnpackOptions.entry`
    );
  }
  if (manifest.entryAsset !== undefined) {
    assert.equal(
      typeof manifest.entryAsset,
      "string",
      `${id}/case.json entryAsset must be a string`
    );
  }
  for (const field of ["expectedErrors", "expectedAssets"] as const) {
    if (manifest[field] !== undefined) {
      assert.ok(
        Array.isArray(manifest[field]) && manifest[field].every((value) => typeof value === "string"),
        `${id}/case.json ${field} must be an array of strings`
      );
    }
  }
  if (manifest.expectedErrorCount !== undefined) {
    assert.equal(
      Number.isInteger(manifest.expectedErrorCount) && manifest.expectedErrorCount >= 0,
      true,
      `${id}/case.json expectedErrorCount must be a non-negative integer`
    );
  }

  return manifest as BundleExecutionCaseManifest;
}

function isEntryOption(entry: unknown): entry is UnpackOptions["entry"] {
  if (typeof entry === "string") {
    return true;
  }
  if (entry == null || typeof entry !== "object" || Array.isArray(entry)) {
    return false;
  }
  return Object.values(entry).every((value) => typeof value === "string");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value != null && typeof value === "object" && !Array.isArray(value);
}
