import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import assert from "node:assert/strict";
import test from "node:test";

import webpack from "webpack";
import type {
  Compiler as WebpackCompiler,
  Configuration as WebpackOptions,
  Stats as WebpackStats,
  Watching as WebpackWatching
} from "webpack";

import unpack from "@unpack-js/core";
import type {
  Compiler as UnpackCompiler,
  Stats as UnpackStats,
  Watching as UnpackWatching
} from "@unpack-js/core";

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

test("observes compiler.close idle and closed compiler run behavior", async () => {
  const webpackFixture = await createFixture("webpack-close-lifecycle-", {
    "src/index.js": "export const value = 1;"
  });
  const unpackFixture = await createFixture("unpack-close-lifecycle-", {
    "src/index.js": "export const value = 1;"
  });
  const webpackCompiler = webpack(webpackOptions(webpackFixture)) as WebpackCompiler;
  const unpackCompiler = unpack(unpackOptions(unpackFixture));

  try {
    const webpackClose = await observeWebpackClose(webpackCompiler);
    assert.equal(webpackClose.calledSynchronously, true);
    assert.equal(webpackClose.err, null);

    const webpackRunAfterClose = await runWebpackCompiler(webpackCompiler);
    assert.equal(webpackRunAfterClose.calledSynchronously, false);
    assert.equal(webpackRunAfterClose.err, null);
    assert.equal(webpackRunAfterClose.hasStats, true);
    assert.equal(webpackRunAfterClose.hasErrors, false);

    const unpackClose = await observeUnpackCompilerClose(unpackCompiler);
    assert.equal(unpackClose.calledSynchronously, false);
    assert.equal(unpackClose.err, null);

    const unpackRunAfterClose = await runUnpackCompiler(unpackCompiler);
    assert.equal(unpackRunAfterClose.calledSynchronously, false);
    assert.equal(unpackRunAfterClose.err?.name, "CompilerClosedError");
    assert.equal(unpackRunAfterClose.hasStats, false);
  } finally {
    await observeWebpackClose(webpackCompiler);
    await rm(webpackFixture, { recursive: true, force: true });
    await rm(unpackFixture, { recursive: true, force: true });
  }
});

test("observes compiler.close while run is active", async () => {
  const webpackFixture = await createFixture("webpack-active-close-lifecycle-", {
    "src/index.js": "export const value = 1;"
  });
  const unpackFixture = await createFixture("unpack-active-close-lifecycle-", {
    "src/index.js": "export const value = 1;"
  });
  const webpackCompiler = webpack(webpackOptions(webpackFixture)) as WebpackCompiler;
  const unpackCompiler = unpack(unpackOptions(unpackFixture));

  try {
    const webpackRun = runWebpackCompiler(webpackCompiler);
    const webpackClose = await observeWebpackClose(webpackCompiler);
    assert.equal(webpackClose.calledSynchronously, true);
    assert.equal(webpackClose.err, null);
    const webpackRunResult = await webpackRun;
    assert.equal(webpackRunResult.err, null);
    assert.equal(webpackRunResult.hasStats, true);
    assert.equal(webpackRunResult.hasErrors, false);

    const unpackRun = runUnpackCompiler(unpackCompiler);
    const unpackClose = await observeUnpackCompilerClose(unpackCompiler);
    assert.equal(unpackClose.calledSynchronously, false);
    assert.equal(unpackClose.err?.name, "CompilerRunningError");
    const unpackRunResult = await unpackRun;
    assert.equal(unpackRunResult.err, null);
    assert.equal(unpackRunResult.hasStats, true);
    assert.equal(unpackRunResult.hasErrors, false);
  } finally {
    await observeWebpackClose(webpackCompiler);
    await observeUnpackCompilerClose(unpackCompiler);
    await rm(webpackFixture, { recursive: true, force: true });
    await rm(unpackFixture, { recursive: true, force: true });
  }
});

test("observes compiler.watch initial callback and conflict behavior", async () => {
  const webpackFixture = await createFixture("webpack-watch-lifecycle-", {
    "src/index.js": "export const value = 1;"
  });
  const unpackFixture = await createFixture("unpack-watch-lifecycle-", {
    "src/index.js": "export const value = 1;"
  });
  const webpackCompiler = webpack(webpackOptions(webpackFixture)) as WebpackCompiler;
  const unpackCompiler = unpack(unpackOptions(unpackFixture));
  let webpackWatching: WebpackWatching | undefined;
  let unpackWatching: UnpackWatching | undefined;
  let unpackSecondWatching: UnpackWatching | undefined;

  try {
    const webpackWatch = observeWebpackWatch(webpackCompiler, "webpack initial watch");
    webpackWatching = webpackWatch.watching;
    assert.equal(webpackWatch.returnedWatching, true);
    const webpackFirst = await webpackWatch.next();
    assert.equal(webpackFirst.calledSynchronously, false);
    assert.equal(webpackFirst.err, null);
    assert.equal(webpackFirst.hasStats, true);
    assert.equal(webpackFirst.hasErrors, false);

    const unpackWatch = observeUnpackWatch(unpackCompiler, "unpack initial watch");
    unpackWatching = unpackWatch.watching;
    assert.equal(unpackWatch.returnedWatching, true);
    const unpackFirst = await unpackWatch.next();
    assert.equal(unpackFirst.calledSynchronously, false);
    assert.equal(unpackFirst.err, null);
    assert.equal(unpackFirst.hasStats, true);
    assert.equal(unpackFirst.hasErrors, false);

    const webpackRunConflict = await runWebpackCompiler(webpackCompiler);
    assert.equal(webpackRunConflict.calledSynchronously, true);
    assert.equal(webpackRunConflict.err?.name, "ConcurrentCompilationError");
    assert.equal(webpackRunConflict.hasStats, false);

    const unpackRunConflict = await runUnpackCompiler(unpackCompiler);
    assert.equal(unpackRunConflict.calledSynchronously, false);
    assert.equal(unpackRunConflict.err?.name, "ConcurrentRunError");
    assert.equal(unpackRunConflict.hasStats, false);

    const webpackSecondWatch = observeWebpackWatch(webpackCompiler, "webpack second watch");
    assert.equal(webpackSecondWatch.returnedWatching, false);
    const webpackSecond = await webpackSecondWatch.next();
    assert.equal(webpackSecond.calledSynchronously, true);
    assert.equal(webpackSecond.err?.name, "ConcurrentCompilationError");
    assert.equal(webpackSecond.hasStats, false);

    const unpackSecondWatch = observeUnpackWatch(unpackCompiler, "unpack second watch");
    unpackSecondWatching = unpackSecondWatch.watching;
    assert.equal(unpackSecondWatch.returnedWatching, true);
    const unpackSecond = await unpackSecondWatch.next();
    assert.equal(unpackSecond.calledSynchronously, false);
    assert.equal(unpackSecond.err?.name, "ConcurrentRunError");
    assert.equal(unpackSecond.hasStats, false);

    const webpackCloseConflict = await observeWebpackClose(webpackCompiler);
    assert.equal(webpackCloseConflict.calledSynchronously, true);
    assert.equal(webpackCloseConflict.err, null);

    const unpackCloseConflict = await observeUnpackCompilerClose(unpackCompiler);
    assert.equal(unpackCloseConflict.calledSynchronously, false);
    assert.equal(unpackCloseConflict.err?.name, "CompilerRunningError");
  } finally {
    if (unpackSecondWatching) {
      await observeUnpackWatchingClose(unpackSecondWatching);
    }
    if (webpackWatching) {
      await observeWebpackClose(webpackWatching);
    }
    if (unpackWatching) {
      await observeUnpackWatchingClose(unpackWatching);
    }
    await observeWebpackClose(webpackCompiler);
    await observeUnpackCompilerClose(unpackCompiler);
    await rm(webpackFixture, { recursive: true, force: true });
    await rm(unpackFixture, { recursive: true, force: true });
  }
});

test("observes Watching.invalidate and close lifecycle", async () => {
  const webpackFixture = await createFixture("webpack-watching-lifecycle-", {
    "src/index.js": "export const value = 'before';"
  });
  const unpackFixture = await createFixture("unpack-watching-lifecycle-", {
    "src/index.js": "export const value = 'before';"
  });
  const webpackCompiler = webpack(webpackOptions(webpackFixture)) as WebpackCompiler;
  const unpackCompiler = unpack(unpackOptions(unpackFixture));
  const webpackWatch = observeWebpackWatch(webpackCompiler, "webpack invalidated watch");
  const unpackWatch = observeUnpackWatch(unpackCompiler, "unpack invalidated watch");

  try {
    assert.equal((await webpackWatch.next()).err, null);
    assert.equal((await unpackWatch.next()).err, null);

    const webpackSecond = webpackWatch.next();
    const unpackSecond = unpackWatch.next();
    await writeFile(join(webpackFixture, "src/index.js"), "export const value = 'after';", {
      encoding: "utf8"
    });
    await writeFile(join(unpackFixture, "src/index.js"), "export const value = 'after';", {
      encoding: "utf8"
    });
    webpackWatch.watching?.invalidate();
    unpackWatch.watching.invalidate();

    const webpackInvalidated = await webpackSecond;
    assert.equal(webpackInvalidated.calledSynchronously, false);
    assert.equal(webpackInvalidated.err, null);
    assert.equal(webpackInvalidated.hasStats, true);
    assert.equal(webpackInvalidated.hasErrors, false);

    const unpackInvalidated = await unpackSecond;
    assert.equal(unpackInvalidated.calledSynchronously, false);
    assert.equal(unpackInvalidated.err, null);
    assert.equal(unpackInvalidated.hasStats, true);
    assert.equal(unpackInvalidated.hasErrors, false);

    const webpackWatchClose = await observeWebpackClose(webpackWatch.watching);
    assert.equal(webpackWatchClose.calledSynchronously, true);
    assert.equal(webpackWatchClose.err, null);

    const unpackWatchClose = await observeUnpackWatchingClose(unpackWatch.watching);
    assert.equal(unpackWatchClose.calledSynchronously, false);
    assert.equal(unpackWatchClose.err, null);

    const webpackRunAfterWatchClose = await runWebpackCompiler(webpackCompiler);
    assert.equal(webpackRunAfterWatchClose.err, null);
    assert.equal(webpackRunAfterWatchClose.hasErrors, false);

    const unpackRunAfterWatchClose = await runUnpackCompiler(unpackCompiler);
    assert.equal(unpackRunAfterWatchClose.err, null);
    assert.equal(unpackRunAfterWatchClose.hasErrors, false);
  } finally {
    await observeWebpackClose(webpackWatch.watching);
    await observeUnpackWatchingClose(unpackWatch.watching);
    await observeWebpackClose(webpackCompiler);
    await observeUnpackCompilerClose(unpackCompiler);
    await rm(webpackFixture, { recursive: true, force: true });
    await rm(unpackFixture, { recursive: true, force: true });
  }
});

test("observes Stats.hasErrors and normalized toJson subset", async () => {
  const webpackSuccessFixture = await createFixture("webpack-stats-success-", {
    "src/index.js": "export const value = 1;"
  });
  const unpackSuccessFixture = await createFixture("unpack-stats-success-", {
    "src/index.js": "export const value = 1;"
  });
  const webpackFailureFixture = await createFixture("webpack-stats-failure-", {
    "src/index.js": "import {"
  });
  const unpackFailureFixture = await createFixture("unpack-stats-failure-", {
    "src/index.js": "import {"
  });
  const webpackSuccessCompiler = webpack(
    webpackOptions(webpackSuccessFixture)
  ) as WebpackCompiler;
  const unpackSuccessCompiler = unpack(unpackOptions(unpackSuccessFixture));
  const webpackFailureCompiler = webpack(
    webpackOptions(webpackFailureFixture)
  ) as WebpackCompiler;
  const unpackFailureCompiler = unpack(unpackOptions(unpackFailureFixture));

  try {
    const webpackSuccess = await runWebpackCompiler(webpackSuccessCompiler);
    const unpackSuccess = await runUnpackCompiler(unpackSuccessCompiler);
    assert.equal(webpackSuccess.err, null);
    assert.equal(webpackSuccess.hasErrors, false);
    assert.equal(unpackSuccess.err, null);
    assert.equal(unpackSuccess.hasErrors, false);

    const webpackSuccessJson = statsJsonObservation(webpackSuccess.stats);
    const unpackSuccessJson = statsJsonObservation(unpackSuccess.stats);
    assert.deepEqual(webpackSuccessJson.assetNames, ["main.js"]);
    assert.deepEqual(unpackSuccessJson.assetNames, ["main.js"]);
    assert.equal(webpackSuccessJson.errorsLength, 0);
    assert.equal(unpackSuccessJson.errorsLength, 0);
    assert.equal(webpackSuccessJson.warningsLength, 0);
    assert.equal(unpackSuccessJson.warningsLength, 0);
    assert.equal(webpackSuccessJson.hasOutputPath, true);
    assert.equal(unpackSuccessJson.hasOutputPath, true);

    assert.ok(webpackSuccessJson.keys.includes("hash"));
    assert.ok(webpackSuccessJson.keys.includes("version"));
    assert.equal(webpackSuccessJson.hasWatchDependencies, false);
    assert.ok(unpackSuccessJson.keys.includes("assets"));
    assert.ok(unpackSuccessJson.keys.includes("errors"));
    assert.ok(unpackSuccessJson.keys.includes("outputPath"));
    assert.ok(unpackSuccessJson.keys.includes("warnings"));
    assert.equal(unpackSuccessJson.keys.includes("hash"), false);
    assert.equal(unpackSuccessJson.keys.includes("version"), false);
    assert.equal(unpackSuccessJson.hasWatchDependencies, true);

    const webpackFailure = await runWebpackCompiler(webpackFailureCompiler);
    const unpackFailure = await runUnpackCompiler(unpackFailureCompiler);
    assert.equal(webpackFailure.err, null);
    assert.equal(webpackFailure.hasErrors, true);
    assert.equal(unpackFailure.err, null);
    assert.equal(unpackFailure.hasErrors, true);

    const webpackFailureJson = statsJsonObservation(webpackFailure.stats);
    const unpackFailureJson = statsJsonObservation(unpackFailure.stats);
    assert.deepEqual(webpackFailureJson.assetNames, ["main.js"]);
    assert.deepEqual(unpackFailureJson.assetNames, ["main.js"]);
    assert.equal(webpackFailureJson.errorsLength, 1);
    assert.equal(unpackFailureJson.errorsLength, 1);
    assert.equal(webpackFailureJson.warningsLength, 0);
    assert.equal(unpackFailureJson.warningsLength, 0);
    assert.equal(webpackFailureJson.hasOutputPath, true);
    assert.equal(unpackFailureJson.hasOutputPath, true);
  } finally {
    await observeWebpackClose(webpackSuccessCompiler);
    await observeUnpackCompilerClose(unpackSuccessCompiler);
    await observeWebpackClose(webpackFailureCompiler);
    await observeUnpackCompilerClose(unpackFailureCompiler);
    await rm(webpackSuccessFixture, { recursive: true, force: true });
    await rm(unpackSuccessFixture, { recursive: true, force: true });
    await rm(webpackFailureFixture, { recursive: true, force: true });
    await rm(unpackFailureFixture, { recursive: true, force: true });
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

async function observeWebpackClose(
  target: WebpackCompiler | WebpackWatching | undefined
) {
  if (!target) {
    return { calledSynchronously: true, err: null } satisfies CloseObservation;
  }

  let calledSynchronously = true;
  const result = await new Promise<CloseObservation>((resolve) => {
    target.close((err) => {
      resolve(closeObservation(calledSynchronously, err));
    });
    calledSynchronously = false;
  });
  return result;
}

async function observeUnpackCompilerClose(compiler: UnpackCompiler) {
  let calledSynchronously = true;
  const result = await new Promise<CloseObservation>((resolve) => {
    compiler.close((err) => {
      resolve(closeObservation(calledSynchronously, err));
    });
    calledSynchronously = false;
  });
  return result;
}

async function observeUnpackWatchingClose(watching: UnpackWatching) {
  let calledSynchronously = true;
  const result = await new Promise<CloseObservation>((resolve) => {
    watching.close((err) => {
      resolve(closeObservation(calledSynchronously, err));
    });
    calledSynchronously = false;
  });
  return result;
}

function observeWebpackWatch(compiler: WebpackCompiler, label: string) {
  let calledSynchronously = true;
  const observer = createWatchObserver(label);
  const watching = compiler.watch({}, (err, stats) => {
    observer.push(runObservation(calledSynchronously, err, stats));
  }) as WebpackWatching | undefined;
  calledSynchronously = false;

  return {
    returnedWatching: watching !== undefined,
    watching,
    next: observer.next,
    calls: observer.calls
  };
}

function observeUnpackWatch(compiler: UnpackCompiler, label: string) {
  let calledSynchronously = true;
  const observer = createWatchObserver(label);
  const watching = compiler.watch({}, (err, stats) => {
    observer.push(runObservation(calledSynchronously, err, stats));
  });
  calledSynchronously = false;

  return {
    returnedWatching: true,
    watching,
    next: observer.next,
    calls: observer.calls
  };
}

function createWatchObserver(label: string) {
  const results: RunObservation[] = [];
  const resolvers: Array<(result: RunObservation) => void> = [];
  let calls = 0;

  return {
    push: (result: RunObservation) => {
      calls += 1;
      const resolve = resolvers.shift();
      if (resolve) {
        resolve(result);
      } else {
        results.push(result);
      }
    },
    next: () => {
      if (results.length > 0) {
        return Promise.resolve(results.shift() as RunObservation);
      }

      return withTimeout(
        new Promise<RunObservation>((resolve) => {
          resolvers.push(resolve);
        }),
        5_000,
        label
      );
    },
    calls: () => calls
  };
}

function statsJsonObservation(stats: WebpackStats | UnpackStats | undefined) {
  assert.ok(stats);
  const json = stats.toJson() as {
    assets?: Array<{ name?: string }>;
    errors?: unknown[];
    warnings?: unknown[];
    outputPath?: unknown;
    watchDependencies?: unknown;
  };

  return {
    keys: Object.keys(json).sort(),
    assetNames: (json.assets ?? [])
      .map((asset) => asset.name)
      .filter((name): name is string => typeof name === "string")
      .sort(),
    errorsLength: json.errors?.length ?? 0,
    warningsLength: json.warnings?.length ?? 0,
    hasOutputPath: typeof json.outputPath === "string" && json.outputPath.length > 0,
    hasWatchDependencies: json.watchDependencies !== undefined
  };
}

interface RunObservation {
  calledSynchronously: boolean;
  err: Error | null;
  stats: WebpackStats | UnpackStats | undefined;
  hasStats: boolean;
  hasErrors: boolean | undefined;
}

interface CloseObservation {
  calledSynchronously: boolean;
  err: Error | null;
}

function runObservation(
  calledSynchronously: boolean,
  err: Error | null | undefined,
  stats: WebpackStats | UnpackStats | undefined
): RunObservation {
  return {
    calledSynchronously,
    err: err ?? null,
    stats,
    hasStats: stats !== undefined,
    hasErrors: stats?.hasErrors()
  };
}

function closeObservation(
  calledSynchronously: boolean,
  err: Error | null | undefined
): CloseObservation {
  return {
    calledSynchronously,
    err: err ?? null
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

async function withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T> {
  let timeout: ReturnType<typeof setTimeout> | undefined;
  const timeoutPromise = new Promise<T>((_, reject) => {
    timeout = setTimeout(() => {
      reject(new Error(`${label} did not call back within ${ms}ms`));
    }, ms);
  });

  try {
    return await Promise.race([promise, timeoutPromise]);
  } finally {
    if (timeout !== undefined) {
      clearTimeout(timeout);
    }
  }
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
