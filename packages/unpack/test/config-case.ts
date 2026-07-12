import assert from "node:assert/strict";
import { access, cp, mkdtemp, readdir, rm } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import test from "node:test";

import unpack from "@unpack-js/core";
import type { Compiler, Stats, UnpackOptions } from "@unpack-js/core";

export type ConfigCaseOptions = Omit<UnpackOptions, "context" | "entry" | "output"> & {
  entry?: UnpackOptions["entry"];
};

export interface ConfigCaseFixture {
  fixturePath: string;
  outputPath: string;
}

export interface ConfigCaseResult extends ConfigCaseFixture {
  outputFiles: string[];
  requireEntry(asset?: string): unknown;
  stats: Stats;
}

export interface ConfigCaseTest {
  prepare?(fixture: ConfigCaseFixture): Promise<void> | void;
  validate(result: ConfigCaseResult): Promise<void> | void;
}

interface TestCase {
  category: string;
  name: string;
  sourcePath: string;
  compiledPath: string;
}

const testDirectory = dirname(fileURLToPath(import.meta.url));
const sourceCasesPath = join(testDirectory, "..", "..", "test", "configCases");
const compiledCasesPath = join(testDirectory, "configCases");
const require = createRequire(import.meta.url);

export async function registerConfigCases(): Promise<void> {
  const cases = await discoverConfigCases();

  assert.notEqual(cases.length, 0, "expected at least one test case");

  for (const testCase of cases) {
    test(`config case ${testCase.category}/${testCase.name}`, async () => {
      await runCase(testCase);
    });
  }
}

async function discoverConfigCases(): Promise<TestCase[]> {
  const categories = await readDirectories(sourceCasesPath);
  const cases = await Promise.all(
    categories.map(async (category) => {
      const names = await readDirectories(join(sourceCasesPath, category));
      return names.map((name) => ({
        category,
        name,
        sourcePath: join(sourceCasesPath, category, name),
        compiledPath: join(compiledCasesPath, category, name)
      }));
    })
  );

  return cases.flat();
}

async function fileExists(path: string): Promise<boolean> {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

async function readDirectories(path: string): Promise<string[]> {
  return (await readdir(path, { withFileTypes: true }))
    .filter((entry) => entry.isDirectory() && !entry.name.startsWith("_"))
    .map((entry) => entry.name)
    .sort();
}

async function runCase(testCase: TestCase): Promise<void> {
  const fixturePath = await mkdtemp(join(tmpdir(), "unpack-test-case-"));
  const outputPath = join(fixturePath, "dist");
  let compiler: Compiler | undefined;

  try {
    await copyFixture(testCase.sourcePath, fixturePath);

    const [options, caseTest] = await Promise.all([
      loadOptions(testCase),
      loadCaseTest(testCase)
    ]);
    const normalizedOptions = withHarnessDefaults(options, fixturePath, outputPath);

    await caseTest.prepare?.({ fixturePath, outputPath });
    compiler = unpack(normalizedOptions);
    const stats = await runCompiler(compiler);
    const errors = stats.toJson().errors.map((error) => error.message).join("\n");

    assert.equal(stats.hasErrors(), false, errors || "unexpected config case errors");

    const outputFiles = (await readdir(outputPath)).sort();
    await caseTest.validate({
      fixturePath,
      outputPath,
      outputFiles,
      requireEntry(asset = "main.js") {
        const entryPath = join(outputPath, asset);
        delete require.cache[entryPath];
        return require(entryPath) as unknown;
      },
      stats
    });
  } finally {
    if (compiler !== undefined) {
      await closeCompiler(compiler);
    }
    await rm(fixturePath, { recursive: true, force: true });
  }
}

async function loadOptions(testCase: TestCase): Promise<ConfigCaseOptions> {
  const optionsModule = (await import(
    pathToFileURL(join(testCase.compiledPath, "webpack.config.js")).href
  )) as { default: ConfigCaseOptions };
  return optionsModule.default;
}

async function loadCaseTest(testCase: TestCase): Promise<ConfigCaseTest> {
  if (!(await fileExists(join(testCase.sourcePath, "test.config.ts")))) {
    return {
      validate({ requireEntry }) {
        requireEntry();
      }
    };
  }

  const testModule = (await import(
    pathToFileURL(join(testCase.compiledPath, "test.config.js")).href
  )) as { default: ConfigCaseTest };
  return testModule.default;
}

function withHarnessDefaults(
  options: ConfigCaseOptions,
  fixturePath: string,
  outputPath: string
): UnpackOptions {
  return {
    entry: "./index.js",
    sourcemap: false,
    ...options,
    context: fixturePath,
    output: { path: outputPath }
  };
}

async function copyFixture(sourcePath: string, fixturePath: string): Promise<void> {
  const entries = await readdir(sourcePath, { withFileTypes: true });

  await Promise.all(
    entries
      .filter(
        (entry) =>
          entry.name !== "webpack.config.ts" &&
          entry.name !== "test.config.ts" &&
          entry.name !== "README.md"
      )
      .map((entry) =>
        cp(join(sourcePath, entry.name), join(fixturePath, entry.name), {
          recursive: true
        })
      )
  );
}

function runCompiler(compiler: Compiler): Promise<Stats> {
  return new Promise((resolve, reject) => {
    compiler.run((error, stats) => {
      if (error) {
        reject(error);
      } else if (stats === undefined) {
        reject(new Error("config case completed without Stats"));
      } else {
        resolve(stats);
      }
    });
  });
}

function closeCompiler(compiler: Compiler): Promise<void> {
  return new Promise((resolve, reject) => {
    compiler.close((error) => (error ? reject(error) : resolve()));
  });
}
