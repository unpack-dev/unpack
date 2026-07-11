import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import assert from "node:assert/strict";
import test from "node:test";

import webpack from "webpack";
import type {
  Compiler as WebpackCompiler,
  Configuration as WebpackOptions,
  Stats as WebpackStats
} from "webpack";

import unpack from "@unpack-js/core";
import type { Compiler as UnpackCompiler, Stats as UnpackStats } from "@unpack-js/core";

test("observes top-level callback validation error timing", async () => {
  const webpackFixture = await createFixture("webpack-validation-lifecycle-", {
    "src/index.js": "export const value = 1;"
  });
  const unpackFixture = await createFixture("unpack-validation-lifecycle-", {
    "src/index.js": "export const value = 1;"
  });

  try {
    const webpackNoCallbackError = captureSynchronousThrow(() =>
      webpack({
        context: webpackFixture,
        entry: "./src/index.js",
        mode: "staging" as WebpackOptions["mode"]
      })
    );
    const webpackValidation = await observeWebpackInvalidModeCallback(webpackFixture);
    const unpackNoCallbackError = captureSynchronousThrow(() =>
      unpack({
        context: unpackFixture,
        entry: "./src/index.js",
        // @ts-expect-error intentionally observing runtime validation
        mode: "staging"
      })
    );
    const unpackValidation = await observeUnpackInvalidModeCallback(unpackFixture);

    assert.equal(webpackNoCallbackError?.name, "ValidationError");
    assert.match(webpackNoCallbackError?.message ?? "", /Invalid configuration object/);
    assert.equal(webpackValidation.returnedCompiler, false);
    assert.equal(webpackValidation.calledSynchronously, false);
    assert.equal(webpackValidation.err?.name, "ValidationError");
    assert.match(webpackValidation.err?.message ?? "", /Invalid configuration object/);
    assert.equal(webpackValidation.hasStats, false);
    assert.equal(unpackNoCallbackError?.name, "TypeError");
    assert.match(unpackNoCallbackError?.message ?? "", /options.mode must be/);
    assert.equal(unpackValidation.returnedCompiler, false);
    assert.equal(unpackValidation.calledSynchronously, false);
    assert.equal(unpackValidation.err?.name, "TypeError");
    assert.match(unpackValidation.err?.message ?? "", /options.mode must be/);
    assert.equal(unpackValidation.hasStats, false);
  } finally {
    await rm(webpackFixture, { recursive: true, force: true });
    await rm(unpackFixture, { recursive: true, force: true });
  }
});

test("delivers top-level Compiler initialization failures asynchronously", async () => {
  const fixture = await createFixture("unpack-initialization-lifecycle-", {
    "src/index.js": "export const value = 1;"
  });

  try {
    const observation = await observeUnpackInitializationFailureCallback(fixture);

    assert.equal(observation.returnedCompiler, false);
    assert.equal(observation.calledSynchronously, false);
    assert.equal(observation.err?.name, "InfrastructureError");
    assert.match(observation.err?.message ?? "", /not supported by Rust regex/);
    assert.equal(observation.hasStats, false);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("observes top-level callback timing and returned compiler lifecycle", async () => {
  const webpackFixture = await createFixture("webpack-lifecycle-", {
    "src/index.js": "export const value = 1;"
  });
  const unpackFixture = await createFixture("unpack-lifecycle-", {
    "src/index.js": "export const value = 1;"
  });

  try {
    const webpackObservation = await observeWebpackTopLevelCallback(webpackFixture);
    assert.equal(webpackObservation.calledSynchronously, false);
    assert.equal(webpackObservation.err, null);
    assert.equal(webpackObservation.hasStats, true);
    assert.equal(webpackObservation.hasErrors, false);

    const webpackRerun = await runWebpackCompiler(webpackObservation.compiler);
    assert.equal(webpackRerun.err, null);
    assert.equal(webpackRerun.hasStats, true);
    assert.equal(webpackRerun.hasErrors, false);
    await closeWebpackCompiler(webpackObservation.compiler);

    const unpackObservation = await observeUnpackTopLevelCallback(unpackFixture);
    assert.equal(unpackObservation.calledSynchronously, false);
    assert.equal(unpackObservation.err, null);
    assert.equal(unpackObservation.hasStats, true);
    assert.equal(unpackObservation.hasErrors, false);

    const unpackRerun = await runUnpackCompiler(unpackObservation.compiler);
    assert.equal(unpackRerun.err, null);
    assert.equal(unpackRerun.hasStats, true);
    assert.equal(unpackRerun.hasErrors, false);
    await closeUnpackCompiler(unpackObservation.compiler);
  } finally {
    await rm(webpackFixture, { recursive: true, force: true });
    await rm(unpackFixture, { recursive: true, force: true });
  }
});

test("aligns compiler.run callback timing and baseline Stats semantics", async () => {
  const webpackFixture = await createFixture("webpack-run-lifecycle-", {
    "src/index.js": "export const value = 1;"
  });
  const unpackFixture = await createFixture("unpack-run-lifecycle-", {
    "src/index.js": "export const value = 1;"
  });
  const webpackCompiler = webpack(webpackOptions(webpackFixture)) as WebpackCompiler;
  const unpackCompiler = unpack(unpackOptions(unpackFixture));

  try {
    const webpackSuccess = await runWebpackCompiler(webpackCompiler);
    assert.equal(webpackSuccess.calledSynchronously, false);
    assert.equal(webpackSuccess.err, null);
    assert.equal(webpackSuccess.hasStats, true);
    assertBaselineWebpackStats(
      webpackSuccess.stats as WebpackStats | undefined,
      join(webpackFixture, "dist"),
      0
    );

    const unpackSuccess = await runUnpackCompiler(unpackCompiler);
    assert.equal(unpackSuccess.calledSynchronously, false);
    assert.equal(unpackSuccess.err, null);
    assert.equal(unpackSuccess.hasStats, true);
    assertBaselineUnpackStats(
      unpackSuccess.stats as UnpackStats | undefined,
      join(unpackFixture, "dist"),
      0
    );

    await Promise.all([
      writeFile(join(webpackFixture, "src/index.js"), "import {", "utf8"),
      writeFile(join(unpackFixture, "src/index.js"), "import {", "utf8")
    ]);

    const webpackError = await runWebpackCompiler(webpackCompiler);
    assert.equal(webpackError.calledSynchronously, false);
    assert.equal(webpackError.err, null);
    assert.equal(webpackError.hasStats, true);
    assertBaselineWebpackStats(
      webpackError.stats as WebpackStats | undefined,
      join(webpackFixture, "dist"),
      1
    );

    const unpackError = await runUnpackCompiler(unpackCompiler);
    assert.equal(unpackError.calledSynchronously, false);
    assert.equal(unpackError.err, null);
    assert.equal(unpackError.hasStats, true);
    assertBaselineUnpackStats(
      unpackError.stats as UnpackStats | undefined,
      join(unpackFixture, "dist"),
      1
    );
  } finally {
    await closeWebpackCompiler(webpackCompiler);
    await closeUnpackCompiler(unpackCompiler);
    await rm(webpackFixture, { recursive: true, force: true });
    await rm(unpackFixture, { recursive: true, force: true });
  }
});

function webpackOptions(context: string): WebpackOptions {
  return {
    context,
    entry: "./src/index.js",
    mode: "none",
    devtool: false,
    output: {
      path: join(context, "dist")
    }
  };
}

function unpackOptions(context: string): Parameters<typeof unpack>[0] {
  return {
    context,
    entry: "./src/index.js",
    mode: "none",
    sourcemap: false,
    output: {
      path: join(context, "dist")
    }
  };
}

async function observeWebpackTopLevelCallback(context: string) {
  let compiler!: WebpackCompiler;
  let calledSynchronously = true;
  const result = await new Promise<RunObservation>((resolve) => {
    compiler = webpack(webpackOptions(context), (err, stats) => {
      resolve(runObservation(calledSynchronously, err ?? null, stats));
    }) as WebpackCompiler;
    calledSynchronously = false;
  });
  return { compiler, ...result };
}

async function observeUnpackTopLevelCallback(context: string) {
  let compiler!: UnpackCompiler;
  let calledSynchronously = true;
  const result = await new Promise<RunObservation>((resolve) => {
    const returnedCompiler = unpack(unpackOptions(context), (err, stats) => {
      resolve(runObservation(calledSynchronously, err, stats));
    });
    assert.ok(returnedCompiler);
    compiler = returnedCompiler;
    calledSynchronously = false;
  });
  return { compiler, ...result };
}

async function observeWebpackInvalidModeCallback(context: string) {
  return observeCallbackEntry((callback) =>
    webpack(
      {
        context,
        entry: "./src/index.js",
        mode: "staging" as WebpackOptions["mode"]
      },
      (err, stats) => {
        callback(err ?? null, stats);
      }
    )
  );
}

async function observeUnpackInvalidModeCallback(context: string) {
  return observeCallbackEntry((callback) =>
    unpack(
      {
        context,
        entry: "./src/index.js",
        // @ts-expect-error intentionally observing runtime validation
        mode: "staging"
      },
      (err, stats) => {
        callback(err, stats);
      }
    )
  );
}

async function observeUnpackInitializationFailureCallback(context: string) {
  return observeCallbackEntry((callback) =>
    unpack(
      {
        context,
        entry: "./src/index.js",
        snapshot: {
          managedPaths: [/(?=node_modules)/]
        }
      },
      (err, stats) => {
        callback(err, stats);
      }
    )
  );
}

async function observeCallbackEntry(
  invoke: (
    callback: (
      err: Error | null | undefined,
      stats: WebpackStats | UnpackStats | undefined
    ) => void
  ) => unknown
) {
  let calledSynchronously = true;
  let returnedCompiler = false;
  const result = await new Promise<RunObservation>((resolve) => {
    const compiler = invoke((err, stats) => {
      resolve(runObservation(calledSynchronously, err, stats));
    });
    returnedCompiler = compiler != null;
    calledSynchronously = false;
  });
  return { returnedCompiler, ...result };
}

async function runWebpackCompiler(compiler: WebpackCompiler) {
  let calledSynchronously = true;
  const result = await new Promise<RunObservation>((resolve) => {
    compiler.run((err, stats) => {
      resolve(runObservation(calledSynchronously, err, stats));
    });
    calledSynchronously = false;
  });
  return result;
}

async function runUnpackCompiler(compiler: UnpackCompiler) {
  let calledSynchronously = true;
  const result = await new Promise<RunObservation>((resolve) => {
    compiler.run((err, stats) => {
      resolve(runObservation(calledSynchronously, err, stats));
    });
    calledSynchronously = false;
  });
  return result;
}

async function closeWebpackCompiler(compiler: WebpackCompiler) {
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

async function closeUnpackCompiler(compiler: UnpackCompiler) {
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

interface RunObservation {
  calledSynchronously: boolean;
  err: Error | null;
  hasStats: boolean;
  hasErrors: boolean | undefined;
  stats: WebpackStats | UnpackStats | undefined;
}

function runObservation(
  calledSynchronously: boolean,
  err: Error | null | undefined,
  stats: WebpackStats | UnpackStats | undefined
): RunObservation {
  return {
    calledSynchronously,
    err: err ?? null,
    hasStats: stats !== undefined,
    hasErrors: stats?.hasErrors(),
    stats
  };
}

function assertBaselineWebpackStats(
  stats: WebpackStats | undefined,
  outputPath: string,
  errorCount: number
): void {
  assert.ok(stats);
  const json = stats.toJson({
    all: false,
    assets: true,
    errors: true,
    outputPath: true,
    warnings: true
  });
  assertBaselineStats(stats.hasErrors(), json, outputPath, errorCount);
}

function assertBaselineUnpackStats(
  stats: UnpackStats | undefined,
  outputPath: string,
  errorCount: number
): void {
  assert.ok(stats);
  assertBaselineStats(stats.hasErrors(), stats.toJson(), outputPath, errorCount);
}

function assertBaselineStats(
  hasErrors: boolean,
  json: {
    assets?: readonly { name?: string; size?: number }[];
    errors?: readonly unknown[];
    outputPath?: string;
    warnings?: readonly unknown[];
  },
  outputPath: string,
  errorCount: number
): void {
  assert.equal(hasErrors, errorCount > 0);
  assert.equal(json.errors?.length, errorCount);
  assert.equal(json.warnings?.length, 0);
  assert.equal(json.outputPath, outputPath);
  const asset = json.assets?.find(({ name }) => name === "main.js");
  assert.ok(asset);
  assert.equal(Number.isInteger(asset.size), true);
  assert.ok((asset.size ?? 0) > 0);

  if (errorCount > 0) {
    const [error] = json.errors ?? [];
    const message =
      typeof error === "object" && error !== null && "message" in error
        ? (error as { message?: unknown }).message
        : error;
    assert.ok(typeof message === "string");
    assert.ok(message.length > 0);
  }
}

function captureSynchronousThrow(callback: () => unknown): Error | null {
  try {
    callback();
    return null;
  } catch (error) {
    assert.ok(error instanceof Error);
    return error;
  }
}

async function delay(ms: number) {
  await new Promise<void>((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function createFixture(prefix: string, files: Record<string, string>) {
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
