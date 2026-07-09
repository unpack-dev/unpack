import { execFile } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { promisify } from "node:util";
import test from "node:test";
import assert from "node:assert/strict";

import unpack from "@unpack-js/core";
import type { Stats } from "@unpack-js/core";

const execFileAsync = promisify(execFile);

test("emitted bundle executes entry and async chunk", async () => {
  const fixture = await createFixture({
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
  });
  const outputPath = join(fixture, "dist");

  try {
    const { err, stats } = await runCompiler({
      context: fixture,
      entry: "./src/index.js",
      output: { path: outputPath },
      sourcemap: false
    });

    assert.equal(err, null);
    assert.equal(stats?.hasErrors(), false);

    const result = await executeBundle(
      outputPath,
      `
        const entry = require("./main.js");

        entry.run()
          .then((value) => {
            console.log(JSON.stringify(value));
          })
          .catch((error) => {
            console.error(error && error.stack || error);
            process.exit(1);
          });
      `
    );

    assert.equal(result, JSON.stringify("entry:async:feature-ok"));
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

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

async function runCompiler(options: Parameters<typeof unpack>[0]) {
  return new Promise<{ err: Error | null; stats?: Stats }>((resolve) => {
    unpack(options, (err, stats) => {
      resolve({ err, stats });
    });
  });
}

async function executeBundle(outputPath: string, script: string) {
  try {
    const { stdout } = await execFileAsync(process.execPath, ["-e", script], {
      cwd: outputPath,
      encoding: "utf8"
    });
    return stdout.trim();
  } catch (error) {
    const result = error as { stdout?: string; stderr?: string };
    assert.fail(`node failed\nstdout:\n${result.stdout ?? ""}\nstderr:\n${result.stderr ?? ""}`);
  }
}
