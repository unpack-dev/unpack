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

interface DefaultCase {
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

interface DefaultCaseManifest {
  entry?: UnpackOptions["entry"];
  entryAsset?: string;
  runtimeExpression: string;
  expected: unknown;
  expectedErrors?: string[];
  expectedErrorCount?: number;
  expectedAssets?: string[];
}

const defaultCases = await readDefaultCases();

for (const defaultCase of defaultCases) {
  test(`default case ${defaultCase.id}`, async () => {
    await runDefaultCase(defaultCase);
  });
}

async function runDefaultCase(defaultCase: DefaultCase) {
  const fixture = await createFixture(defaultCase.path);
  const outputPath = join(fixture, "dist");

  try {
    const { err, stats } = await runCompiler({
      ...defaultCompilerOptions,
      context: fixture,
      entry: defaultCase.entry ?? defaultEntry,
      output: { path: outputPath }
    });

    assert.equal(err, null);
    const errors = stats?.toJson().errors ?? [];
    if (defaultCase.expectedErrors === undefined && defaultCase.expectedErrorCount === undefined) {
      assert.equal(stats?.hasErrors(), false);
    } else {
      if (defaultCase.expectedErrors !== undefined) {
        assert.equal(stats?.hasErrors(), true);
        for (const expectedError of defaultCase.expectedErrors) {
          assert.ok(
            errors.some((error) => error.message.includes(expectedError)),
            `expected Stats error containing ${JSON.stringify(expectedError)}`
          );
        }
      }
      if (defaultCase.expectedErrorCount !== undefined) {
        assert.equal(errors.length, defaultCase.expectedErrorCount);
      }
    }
    assert.ok(
      stats?.toJson().assets.some((asset) => asset.name === (defaultCase.entryAsset ?? defaultEntryAsset))
    );
    assert.ok(
      (await readdir(outputPath)).includes(defaultCase.entryAsset ?? defaultEntryAsset)
    );
    if (defaultCase.expectedAssets !== undefined) {
      assert.deepEqual(
        stats?.toJson().assets.map((asset) => asset.name).sort(),
        [...defaultCase.expectedAssets].sort()
      );
    }

    const result = await executeEntryExpression(
      outputPath,
      defaultCase.entryAsset ?? defaultEntryAsset,
      defaultCase.runtimeExpression
    );

    assert.equal(result, JSON.stringify(defaultCase.expected));
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
}

async function readDefaultCases() {
  const entries = await readdir(casesRoot, { withFileTypes: true });
  const caseDirectories = entries
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();

  assert.notEqual(caseDirectories.length, 0, "expected at least one default case");

  return Promise.all(
    caseDirectories.map(async (id): Promise<DefaultCase> => {
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

function parseCaseManifest(id: string, source: string): DefaultCaseManifest {
  const parsed = JSON.parse(source) as unknown;

  assert.ok(isRecord(parsed), `${id}/case.json must define an object`);

  const manifest = parsed as Partial<DefaultCaseManifest>;

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

  return manifest as DefaultCaseManifest;
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
