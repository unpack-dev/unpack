import { execFile } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { promisify } from "node:util";
import test from "node:test";
import assert from "node:assert/strict";

import unpack from "@unpack-js/core";
import type { Stats, UnpackOptions } from "@unpack-js/core";

const execFileAsync = promisify(execFile);
const defaultEntry = "./src/index.js";
const defaultEntryAsset = "main.js";
const defaultCompilerOptions = {
  sourcemap: false
} satisfies Pick<UnpackOptions, "sourcemap">;

interface BundleExecutionCase {
  name: string;
  files: Record<string, string>;
  entry?: UnpackOptions["entry"];
  entryAsset?: string;
  runtimeExpression: string;
  expected: unknown;
}

const bundleExecutionCases: BundleExecutionCase[] = [
  {
    name: "preserves static ESM live bindings",
    files: {
      "src/index.js": `
        import { value, setValue } from "./state";

        export function run() {
          const before = value;
          setValue(7);
          return [before, value];
        }
      `,
      "src/state.js": `
        export let value = 1;

        export function setValue(next) {
          value = next;
        }
      `
    },
    runtimeExpression: "entry.run()",
    expected: [1, 7]
  },
  {
    name: "loads async chunks",
    files: {
      "src/index.js": `
        import { label } from "./label";

        export async function run() {
          const feature = await import("./feature");
          return [label, feature.value, feature.describe("ok")].join(":");
        }
      `,
      "src/label.js": `
        export const label = "entry";
      `,
      "src/feature.js": `
        export const value = "async";

        export function describe(suffix) {
          return "feature-" + suffix;
        }
      `
    },
    runtimeExpression: "entry.run()",
    expected: "entry:async:feature-ok"
  }
];

for (const bundleCase of bundleExecutionCases) {
  test(`emitted bundle ${bundleCase.name}`, async () => {
    await runBundleExecutionCase(bundleCase);
  });
}

async function runBundleExecutionCase(bundleCase: BundleExecutionCase) {
  const fixture = await createFixture(bundleCase.files);
  const outputPath = join(fixture, "dist");

  try {
    const { err, stats } = await runCompiler({
      ...defaultCompilerOptions,
      context: fixture,
      entry: bundleCase.entry ?? defaultEntry,
      output: { path: outputPath }
    });

    assert.equal(err, null);
    assert.equal(stats?.hasErrors(), false);

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

async function createFixture(files: Record<string, string>) {
  const root = await mkdtemp(join(tmpdir(), "unpack-e2e-"));
  await Promise.all(
    Object.entries(files).map(async ([path, source]) => {
      const absolutePath = join(root, path);
      await mkdir(dirname(absolutePath), { recursive: true });
      await writeFile(absolutePath, source, { encoding: "utf8" });
    })
  );
  return root;
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
