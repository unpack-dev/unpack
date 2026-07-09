import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import assert from "node:assert/strict";

import webpack from "webpack";
import type {
  Compiler as WebpackCompiler,
  Configuration as WebpackOptions,
  Stats as WebpackStats
} from "webpack";

import unpack from "@unpack-js/core";
import type {
  Compiler as UnpackCompiler,
  Stats as UnpackStats,
  UnpackOptions
} from "@unpack-js/core";

export interface FixtureFiles {
  [path: string]: string;
}

export interface ComparisonFixture {
  webpackRoot: string;
  unpackRoot: string;
  cleanup(): Promise<void>;
}

export interface BuildObservation<TStats extends WebpackStats | UnpackStats> {
  err: Error | null;
  stats: TStats | undefined;
  hasStats: boolean;
  hasErrors: boolean | undefined;
  assets: string[];
  outputPath: string | undefined;
}

export type WebpackBuildObservation = BuildObservation<WebpackStats>;
export type UnpackBuildObservation = BuildObservation<UnpackStats>;

export interface NodeScriptObservation {
  stdout: string;
  stderr: string;
  status: number | null;
  signal: NodeJS.Signals | null;
  error: Error | undefined;
}

interface StatsJsonSubset {
  assets?: Array<{ name?: string }>;
  outputPath?: string;
}

export async function createComparisonFixture(
  prefix: string,
  files: FixtureFiles
): Promise<ComparisonFixture> {
  const webpackRoot = await createFixture(`${prefix}webpack-`, files);
  const unpackRoot = await createFixture(`${prefix}unpack-`, files);

  return {
    webpackRoot,
    unpackRoot,
    async cleanup() {
      await Promise.all([
        rm(webpackRoot, { recursive: true, force: true }),
        rm(unpackRoot, { recursive: true, force: true })
      ]);
    }
  };
}

export function webpackNodeOptions(
  root: string,
  overrides: WebpackOptions = {}
): WebpackOptions {
  const output = overrides.output ?? {};

  return {
    context: root,
    mode: "none",
    target: "node",
    entry: "./src/index.js",
    ...overrides,
    output: {
      path: join(root, "dist"),
      library: {
        type: "commonjs2"
      },
      ...output
    }
  };
}

export function unpackOptions(root: string, overrides: Partial<UnpackOptions> = {}): UnpackOptions {
  return {
    context: root,
    mode: "none",
    entry: "./src/index.js",
    ...overrides,
    output: {
      path: join(root, "dist"),
      ...overrides.output
    }
  };
}

export async function runWebpack(options: WebpackOptions): Promise<WebpackBuildObservation> {
  const compiler = webpack(options) as WebpackCompiler;

  try {
    return await runWebpackCompiler(compiler);
  } finally {
    await closeWebpackCompiler(compiler);
  }
}

export async function runUnpack(options: UnpackOptions): Promise<UnpackBuildObservation> {
  const compiler = unpack(options);

  try {
    return await runUnpackCompiler(compiler);
  } finally {
    await closeUnpackCompiler(compiler);
  }
}

export async function runWebpackCompiler(
  compiler: WebpackCompiler
): Promise<WebpackBuildObservation> {
  const result = await new Promise<{ err: Error | null; stats: WebpackStats | undefined }>(
    (resolve) => {
      compiler.run((err, stats) => {
        resolve({ err: err ?? null, stats });
      });
    }
  );
  return buildObservation(result.err, result.stats);
}

export async function runUnpackCompiler(
  compiler: UnpackCompiler
): Promise<UnpackBuildObservation> {
  const result = await new Promise<{ err: Error | null; stats: UnpackStats | undefined }>(
    (resolve) => {
      compiler.run((err, stats) => {
        resolve({ err, stats });
      });
    }
  );
  return buildObservation(result.err, result.stats);
}

export async function closeWebpackCompiler(compiler: WebpackCompiler): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    compiler.close((err) => {
      if (err) {
        reject(err);
      } else {
        resolve();
      }
    });
  });
}

export async function closeUnpackCompiler(compiler: UnpackCompiler): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    compiler.close((err) => {
      if (err) {
        reject(err);
      } else {
        resolve();
      }
    });
  });
}

export function listAssets(stats: WebpackStats | UnpackStats | undefined): string[] {
  if (!stats) {
    return [];
  }

  return statsJsonSubset(stats)
    .assets?.map((asset) => asset.name)
    .filter((name): name is string => name !== undefined)
    .sort() ?? [];
}

export async function readAsset(root: string, name: string): Promise<string> {
  return await readFile(join(root, "dist", name), "utf8");
}

export async function runNodeScript(
  root: string,
  script: string
): Promise<NodeScriptObservation> {
  return await new Promise<NodeScriptObservation>((resolve) => {
    const child = spawn(process.execPath, ["-e", script], {
      cwd: join(root, "dist"),
      stdio: ["ignore", "pipe", "pipe"]
    });
    let stdout = "";
    let stderr = "";
    let error: Error | undefined;

    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk: string) => {
      stderr += chunk;
    });
    child.on("error", (childError) => {
      error = childError;
    });
    child.on("close", (status, signal) => {
      resolve({ stdout, stderr, status, signal, error });
    });
  });
}

export function captureSynchronousThrow(callback: () => unknown): Error | null {
  try {
    callback();
    return null;
  } catch (error) {
    assert.ok(error instanceof Error);
    return error;
  }
}

export async function delay(ms: number): Promise<void> {
  await new Promise<void>((resolve) => {
    setTimeout(resolve, ms);
  });
}

function buildObservation<TStats extends WebpackStats | UnpackStats>(
  err: Error | null,
  stats: TStats | undefined
): BuildObservation<TStats> {
  const json = statsJsonSubset(stats);

  return {
    err,
    stats,
    hasStats: stats !== undefined,
    hasErrors: stats?.hasErrors(),
    assets: listAssets(stats),
    outputPath: json.outputPath
  };
}

function statsJsonSubset(stats: WebpackStats | UnpackStats | undefined): StatsJsonSubset {
  if (!stats) {
    return {};
  }

  return (stats as { toJson(options?: unknown): StatsJsonSubset }).toJson({
    all: false,
    assets: true,
    outputPath: true
  });
}

async function createFixture(prefix: string, files: FixtureFiles): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), prefix));
  await Promise.all(
    Object.entries(files).map(async ([path, source]) => {
      const file = join(root, path);
      await mkdir(dirname(file), { recursive: true });
      await writeFile(file, source, { encoding: "utf8" });
    })
  );
  return root;
}
