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
    let unpackCallbackCalled = false;
    const unpackNoCallbackError = captureSynchronousThrow(() =>
      unpack({
        context: unpackFixture,
        entry: "./src/index.js",
        // @ts-expect-error intentionally observing runtime validation
        mode: "staging"
      })
    );
    const unpackError = captureSynchronousThrow(() =>
      unpack(
        {
          context: unpackFixture,
          entry: "./src/index.js",
          // @ts-expect-error intentionally observing runtime validation
          mode: "staging"
        },
        () => {
          unpackCallbackCalled = true;
        }
      )
    );

    assert.equal(webpackNoCallbackError?.name, "ValidationError");
    assert.match(webpackNoCallbackError?.message ?? "", /Invalid configuration object/);
    assert.equal(webpackValidation.returnedCompiler, false);
    assert.equal(webpackValidation.calledSynchronously, false);
    assert.equal(webpackValidation.err?.name, "ValidationError");
    assert.match(webpackValidation.err?.message ?? "", /Invalid configuration object/);
    assert.equal(webpackValidation.hasStats, false);
    assert.equal(unpackNoCallbackError?.name, "TypeError");
    assert.match(unpackNoCallbackError?.message ?? "", /options.mode must be/);
    assert.equal(unpackError?.name, "TypeError");
    assert.match(unpackError?.message ?? "", /options.mode must be/);
    await delay(0);
    assert.equal(unpackCallbackCalled, false);
  } finally {
    await rm(webpackFixture, { recursive: true, force: true });
    await rm(unpackFixture, { recursive: true, force: true });
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
    assert.equal(unpackRerun.err?.name, "CompilerClosedError");
    assert.equal(unpackRerun.hasStats, false);
    await closeUnpackCompiler(unpackObservation.compiler);
  } finally {
    await rm(webpackFixture, { recursive: true, force: true });
    await rm(unpackFixture, { recursive: true, force: true });
  }
});

test("observes compiler.run callback timing and parse error stats semantics", async () => {
  const webpackFixture = await createFixture("webpack-run-lifecycle-", {
    "src/index.js": "import {"
  });
  const unpackFixture = await createFixture("unpack-run-lifecycle-", {
    "src/index.js": "import {"
  });
  const webpackCompiler = webpack(webpackOptions(webpackFixture)) as WebpackCompiler;
  const unpackCompiler = unpack(unpackOptions(unpackFixture));

  try {
    const webpackRun = await runWebpackCompiler(webpackCompiler);
    assert.equal(webpackRun.calledSynchronously, false);
    assert.equal(webpackRun.err, null);
    assert.equal(webpackRun.hasStats, true);
    assert.equal(webpackRun.hasErrors, true);

    const unpackRun = await runUnpackCompiler(unpackCompiler);
    assert.equal(unpackRun.calledSynchronously, false);
    assert.equal(unpackRun.err, null);
    assert.equal(unpackRun.hasStats, true);
    assert.equal(unpackRun.hasErrors, true);
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
    compiler = unpack(unpackOptions(context), (err, stats) => {
      resolve(runObservation(calledSynchronously, err, stats));
    });
    calledSynchronously = false;
  });
  return { compiler, ...result };
}

async function observeWebpackInvalidModeCallback(context: string) {
  let calledSynchronously = true;
  let returnedCompiler = false;
  const result = await new Promise<RunObservation>((resolve) => {
    const compiler = webpack(
      {
        context,
        entry: "./src/index.js",
        mode: "staging" as WebpackOptions["mode"]
      },
      (err, stats) => {
        resolve(runObservation(calledSynchronously, err ?? null, stats));
      }
    ) as WebpackCompiler | null | undefined;
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
    hasErrors: stats?.hasErrors()
  };
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
