import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import assert from "node:assert/strict";
import test from "node:test";

import webpack from "webpack";
import type {
  Compilation as WebpackCompilation,
  Compiler as WebpackCompiler,
  Configuration as WebpackOptions,
  Stats as WebpackStats
} from "webpack";

import unpack from "@unpack-js/core";
import type {
  Compilation as UnpackCompilation,
  Compiler as UnpackCompiler,
  Stats as UnpackStats
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

test("keeps the previous Compilation and Stats valid without sharing ModuleGraph across runs", async () => {
  const files = {
    "src/index.js": "import { value } from './old'; export { value };",
    "src/old.js": "export const value = 'old';",
    "src/new.js": "export const value = 'new';"
  };
  const webpackFixture = await createFixture("webpack-compilation-lifetime-", files);
  const unpackFixture = await createFixture("unpack-compilation-lifetime-", files);
  const webpackCompiler = webpack(webpackOptions(webpackFixture)) as WebpackCompiler;
  const unpackCompiler = unpack(unpackOptions(unpackFixture));

  try {
    const webpackObservation = await observeCompilationLifetime<
      WebpackCompilation,
      WebpackStats
    >({
      fixture: webpackFixture,
      tapCompilation: (callback) =>
        webpackCompiler.hooks.compilation.tap("observe compilation lifetime", callback),
      run: () => requireSuccessfulStats(runWebpackCompiler(webpackCompiler)),
      compilationOf: (stats) => stats.compilation,
      moduleGraphOf: (compilation) => compilation.moduleGraph,
      snapshotCompilation: snapshotWebpackCompilation,
      snapshotStats: snapshotWebpackStats
    });
    assertCompilationLifetime(webpackObservation, "/src/old.js", "/src/new.js");

    const unpackObservation = await observeCompilationLifetime<
      UnpackCompilation,
      UnpackStats
    >({
      fixture: unpackFixture,
      tapCompilation: (callback) =>
        unpackCompiler.hooks.compilation.tap("observe compilation lifetime", callback),
      run: () => requireSuccessfulStats(runUnpackCompiler(unpackCompiler)),
      compilationOf: (stats) => stats.compilation,
      moduleGraphOf: (compilation) => compilation.moduleGraph,
      snapshotCompilation: snapshotUnpackCompilation,
      snapshotStats: snapshotUnpackStats
    });
    assertCompilationLifetime(unpackObservation, "/src/old.js", "/src/new.js");
  } finally {
    await closeWebpackCompiler(webpackCompiler);
    await closeUnpackCompiler(unpackCompiler);
    await rm(webpackFixture, { recursive: true, force: true });
    await rm(unpackFixture, { recursive: true, force: true });
  }
});

test("records the concurrent run callback deviation without corrupting the active run", async () => {
  const webpackFixture = await createFixture("webpack-concurrent-run-", {
    "src/index.js": "export const value = 1;"
  });
  const unpackFixture = await createFixture("unpack-concurrent-run-", {
    "src/index.js": "export const value = 1;"
  });
  const webpackCompiler = webpack(webpackOptions(webpackFixture)) as WebpackCompiler;
  const unpackCompiler = unpack(unpackOptions(unpackFixture));

  try {
    const webpackObservation = await observeConcurrentRuns((callback) => {
      webpackCompiler.run((err, stats) => callback(err, stats));
    });
    assert.equal(webpackObservation.first.err, null);
    assert.equal(webpackObservation.first.hasStats, true);
    assert.equal(webpackObservation.firstCalls, 1);
    assert.equal(webpackObservation.conflict.calledSynchronously, true);
    assert.equal(webpackObservation.conflict.err?.name, "ConcurrentCompilationError");
    assert.equal(webpackObservation.conflict.hasStats, false);
    assert.equal(webpackObservation.conflictCalls, 1);

    const unpackObservation = await observeConcurrentRuns((callback) => {
      unpackCompiler.run((err, stats) => callback(err, stats));
    });
    assert.equal(unpackObservation.first.err, null);
    assert.equal(unpackObservation.first.hasStats, true);
    assert.equal(unpackObservation.firstCalls, 1);
    assert.equal(unpackObservation.conflict.calledSynchronously, false);
    assert.equal(unpackObservation.conflict.err?.name, "ConcurrentRunError");
    assert.equal(unpackObservation.conflict.hasStats, false);
    assert.equal(unpackObservation.conflictCalls, 1);

    const laterRun = await runUnpackCompiler(unpackCompiler);
    assert.equal(laterRun.err, null);
    assert.equal(laterRun.hasStats, true);
  } finally {
    await closeWebpackCompiler(webpackCompiler);
    await closeUnpackCompiler(unpackCompiler);
    await rm(webpackFixture, { recursive: true, force: true });
    await rm(unpackFixture, { recursive: true, force: true });
  }
});

test("records the active-run watch conflict deviation without corrupting the run", async () => {
  const webpackFixture = await createFixture("webpack-run-watch-conflict-", {
    "src/index.js": "export const value = 1;"
  });
  const unpackFixture = await createFixture("unpack-run-watch-conflict-", {
    "src/index.js": "export const value = 1;"
  });
  const webpackCompiler = webpack(webpackOptions(webpackFixture)) as WebpackCompiler;
  const unpackCompiler = unpack(unpackOptions(unpackFixture));

  try {
    const webpackObservation = await observeConcurrentRuns(
      (callback) => {
        webpackCompiler.run((err, stats) => callback(err, stats));
      },
      (callback) => {
        webpackCompiler.watch({}, (err, stats) => callback(err, stats));
      }
    );
    assert.equal(webpackObservation.first.err, null);
    assert.equal(webpackObservation.first.hasStats, true);
    assert.equal(webpackObservation.firstCalls, 1);
    assert.equal(webpackObservation.conflict.calledSynchronously, true);
    assert.equal(webpackObservation.conflict.err?.name, "ConcurrentCompilationError");
    assert.equal(webpackObservation.conflict.hasStats, false);
    assert.equal(webpackObservation.conflictCalls, 1);

    const unpackObservation = await observeConcurrentRuns(
      (callback) => {
        unpackCompiler.run((err, stats) => callback(err, stats));
      },
      (callback) => {
        unpackCompiler.watch({}, (err, stats) => callback(err, stats));
      }
    );
    assert.equal(unpackObservation.first.err, null);
    assert.equal(unpackObservation.first.hasStats, true);
    assert.equal(unpackObservation.firstCalls, 1);
    assert.equal(unpackObservation.conflict.calledSynchronously, false);
    assert.equal(unpackObservation.conflict.err?.name, "ConcurrentRunError");
    assert.equal(unpackObservation.conflict.hasStats, false);
    assert.equal(unpackObservation.conflictCalls, 1);

    const laterRun = await runUnpackCompiler(unpackCompiler);
    assert.equal(laterRun.err, null);
    assert.equal(laterRun.hasStats, true);
  } finally {
    await closeWebpackCompiler(webpackCompiler);
    await closeUnpackCompiler(unpackCompiler);
    await rm(webpackFixture, { recursive: true, force: true });
    await rm(unpackFixture, { recursive: true, force: true });
  }
});

test("aligns successful run callback reentry", async () => {
  const webpackFixture = await createFixture("webpack-run-reentry-", {
    "src/index.js": "export const value = 1;"
  });
  const unpackFixture = await createFixture("unpack-run-reentry-", {
    "src/index.js": "export const value = 1;"
  });
  const webpackCompiler = webpack(webpackOptions(webpackFixture)) as WebpackCompiler;
  const unpackCompiler = unpack(unpackOptions(unpackFixture));

  try {
    const webpackObservation = await observeRunReentry((callback) => {
      webpackCompiler.run((err, stats) => callback(err, stats));
    });
    assertRunReentry(webpackObservation);

    const unpackObservation = await observeRunReentry((callback) => {
      unpackCompiler.run((err, stats) => callback(err, stats));
    });
    assertRunReentry(unpackObservation);
  } finally {
    await closeWebpackCompiler(webpackCompiler);
    await closeUnpackCompiler(unpackCompiler);
    await rm(webpackFixture, { recursive: true, force: true });
    await rm(unpackFixture, { recursive: true, force: true });
  }
});

test("aligns infrastructure failure delivery and releases the active run", async () => {
  const webpackFixture = await createFixture("webpack-run-infrastructure-error-", {
    "src/index.js": "export const value = 1;",
    output: "not a directory"
  });
  const unpackFixture = await createFixture("unpack-run-infrastructure-error-", {
    "src/index.js": "export const value = 1;",
    output: "not a directory"
  });
  const webpackCompiler = webpack({
    ...webpackOptions(webpackFixture),
    output: { path: join(webpackFixture, "output") }
  }) as WebpackCompiler;
  const unpackCompiler = unpack({
    ...unpackOptions(unpackFixture),
    output: { path: join(unpackFixture, "output") }
  });

  try {
    const webpackObservation = await observeSingleRun((callback) => {
      webpackCompiler.run((err, stats) => callback(err, stats));
    });
    assert.equal(webpackObservation.calledSynchronously, false);
    assert.ok(webpackObservation.err);
    assert.equal(webpackObservation.hasStats, false);
    assert.equal(webpackObservation.calls, 1);

    const unpackObservation = await observeSingleRun((callback) => {
      unpackCompiler.run((err, stats) => callback(err, stats));
    });
    assert.equal(unpackObservation.calledSynchronously, false);
    assert.equal(unpackObservation.err?.name, "OutputWriteError");
    assert.equal(unpackObservation.hasStats, false);
    assert.equal(unpackObservation.calls, 1);

    const unpackClose = await observeClose((callback) => {
      unpackCompiler.close(callback);
    });
    assert.equal(unpackClose.err, null);
    assert.equal(unpackClose.calls, 1);
  } finally {
    await closeWebpackCompiler(webpackCompiler);
    await closeUnpackCompiler(unpackCompiler);
    await rm(webpackFixture, { recursive: true, force: true });
    await rm(unpackFixture, { recursive: true, force: true });
  }
});

test("records the asynchronous terminal close lifecycle deviation", async () => {
  const webpackFixture = await createFixture("webpack-terminal-close-", {
    "src/index.js": "export const value = 1;"
  });
  const unpackFixture = await createFixture("unpack-terminal-close-", {
    "src/index.js": "export const value = 1;"
  });
  const webpackCompiler = webpack(webpackOptions(webpackFixture)) as WebpackCompiler;
  const unpackCompiler = unpack(unpackOptions(unpackFixture));

  try {
    const webpackClose = await observeClose((callback) => {
      webpackCompiler.close(callback);
    });
    assert.equal(webpackClose.calledSynchronously, true);
    assert.equal(webpackClose.err, null);
    assert.equal(webpackClose.calls, 1);
    const webpackRun = await runWebpackCompiler(webpackCompiler);
    assert.equal(webpackRun.err, null);
    assert.equal(webpackRun.hasStats, true);

    const unpackClose = await observeClose((callback) => {
      unpackCompiler.close(callback);
    });
    assert.equal(unpackClose.calledSynchronously, false);
    assert.equal(unpackClose.err, null);
    assert.equal(unpackClose.calls, 1);
    const unpackRun = await runUnpackCompiler(unpackCompiler);
    assert.equal(unpackRun.calledSynchronously, false);
    assert.equal(unpackRun.err?.name, "CompilerClosedError");
    assert.equal(unpackRun.hasStats, false);

    const webpackRepeatedClose = await observeClose((callback) => {
      webpackCompiler.close(callback);
    });
    assert.equal(webpackRepeatedClose.calledSynchronously, true);
    assert.equal(webpackRepeatedClose.err, null);
    assert.equal(webpackRepeatedClose.calls, 1);
    const unpackRepeatedClose = await observeClose((callback) => {
      unpackCompiler.close(callback);
    });
    assert.equal(unpackRepeatedClose.calledSynchronously, false);
    assert.equal(unpackRepeatedClose.err, null);
    assert.equal(unpackRepeatedClose.calls, 1);
  } finally {
    await closeWebpackCompiler(webpackCompiler);
    await closeUnpackCompiler(unpackCompiler);
    await rm(webpackFixture, { recursive: true, force: true });
    await rm(unpackFixture, { recursive: true, force: true });
  }
});

test("records the close-during-run safety deviation without corrupting the run", async () => {
  const webpackFixture = await createFixture("webpack-active-close-", {
    "src/index.js": "export const value = 1;"
  });
  const unpackFixture = await createFixture("unpack-active-close-", {
    "src/index.js": "export const value = 1;"
  });
  const webpackCompiler = webpack(webpackOptions(webpackFixture)) as WebpackCompiler;
  const unpackCompiler = unpack(unpackOptions(unpackFixture));

  try {
    const webpackRunPromise = runWebpackCompiler(webpackCompiler);
    const webpackClose = await observeClose((callback) => {
      webpackCompiler.close(callback);
    });
    assert.equal(webpackClose.calledSynchronously, true);
    assert.equal(webpackClose.err, null);
    assert.equal(webpackClose.calls, 1);
    const webpackRun = await webpackRunPromise;
    assert.equal(webpackRun.err, null);
    assert.equal(webpackRun.hasStats, true);

    const unpackRunPromise = runUnpackCompiler(unpackCompiler);
    const unpackClose = await observeClose((callback) => {
      unpackCompiler.close(callback);
    });
    assert.equal(unpackClose.calledSynchronously, false);
    assert.equal(unpackClose.err?.name, "CompilerRunningError");
    assert.equal(unpackClose.calls, 1);
    const unpackRun = await unpackRunPromise;
    assert.equal(unpackRun.err, null);
    assert.equal(unpackRun.hasStats, true);

    const finalClose = await observeClose((callback) => {
      unpackCompiler.close(callback);
    });
    assert.equal(finalClose.err, null);
    assert.equal(finalClose.calls, 1);
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

interface CompilationSnapshot {
  moduleIdentifiers: string[];
  outgoingConnectionCount: number;
}

interface StatsSnapshot {
  hasErrors: boolean;
  errorCount: number;
  assets: { name: string | undefined; size: number | undefined }[];
  outputPath: string | undefined;
}

interface CompilationLifetimeObservation {
  distinctCompilations: boolean;
  distinctModuleGraphs: boolean;
  firstStatsReferencesFirstCompilation: boolean;
  secondStatsReferencesSecondCompilation: boolean;
  oldCompilationBeforeSecond: CompilationSnapshot;
  oldCompilationDuringSecond: CompilationSnapshot;
  oldCompilationAfterSecond: CompilationSnapshot;
  newCompilationAfterSecond: CompilationSnapshot;
  oldStatsBeforeSecond: StatsSnapshot;
  oldStatsDuringSecond: StatsSnapshot;
  oldStatsAfterSecond: StatsSnapshot;
}

async function observeCompilationLifetime<TCompilation, TStats>({
  fixture,
  tapCompilation,
  run,
  compilationOf,
  moduleGraphOf,
  snapshotCompilation,
  snapshotStats
}: {
  fixture: string;
  tapCompilation(callback: (compilation: TCompilation) => void): void;
  run(): Promise<TStats>;
  compilationOf(stats: TStats): TCompilation;
  moduleGraphOf(compilation: TCompilation): object;
  snapshotCompilation(compilation: TCompilation): CompilationSnapshot;
  snapshotStats(stats: TStats): StatsSnapshot;
}): Promise<CompilationLifetimeObservation> {
  const compilations: TCompilation[] = [];
  let firstStats: TStats | undefined;
  let oldCompilationDuringSecond: CompilationSnapshot | undefined;
  let oldStatsDuringSecond: StatsSnapshot | undefined;
  tapCompilation((compilation) => {
    if (compilations.length === 1 && firstStats) {
      oldCompilationDuringSecond = snapshotCompilation(compilationOf(firstStats));
      oldStatsDuringSecond = snapshotStats(firstStats);
    }
    compilations.push(compilation);
  });

  firstStats = await run();
  const firstCompilation = compilationOf(firstStats);
  const oldCompilationBeforeSecond = snapshotCompilation(firstCompilation);
  const oldStatsBeforeSecond = snapshotStats(firstStats);

  await writeFile(
    join(fixture, "src/index.js"),
    "import { value } from './new'; export { value };",
    "utf8"
  );

  const secondStats = await run();
  assert.ok(oldCompilationDuringSecond);
  assert.ok(oldStatsDuringSecond);
  const secondCompilation = compilationOf(secondStats);

  return {
    distinctCompilations: firstCompilation !== secondCompilation,
    distinctModuleGraphs:
      moduleGraphOf(firstCompilation) !== moduleGraphOf(secondCompilation),
    firstStatsReferencesFirstCompilation: compilationOf(firstStats) === firstCompilation,
    secondStatsReferencesSecondCompilation:
      compilationOf(secondStats) === secondCompilation,
    oldCompilationBeforeSecond,
    oldCompilationDuringSecond,
    oldCompilationAfterSecond: snapshotCompilation(firstCompilation),
    newCompilationAfterSecond: snapshotCompilation(secondCompilation),
    oldStatsBeforeSecond,
    oldStatsDuringSecond,
    oldStatsAfterSecond: snapshotStats(firstStats)
  };
}

async function requireSuccessfulStats<TStats>(
  observation: Promise<RunObservation>
): Promise<TStats> {
  const result = await observation;
  assert.equal(result.err, null);
  assert.ok(result.stats);
  return result.stats as TStats;
}

function snapshotWebpackCompilation(compilation: WebpackCompilation): CompilationSnapshot {
  const modules = [...compilation.modules];
  return {
    moduleIdentifiers: modules.map(normalizeModuleIdentifier).sort(),
    outgoingConnectionCount: modules.reduce(
      (count, module) =>
        count + [...compilation.moduleGraph.getOutgoingConnections(module)].length,
      0
    )
  };
}

function snapshotUnpackCompilation(compilation: UnpackCompilation): CompilationSnapshot {
  const modules = [...compilation.modules];
  return {
    moduleIdentifiers: modules.map(normalizeModuleIdentifier).sort(),
    outgoingConnectionCount: modules.reduce(
      (count, module) =>
        count + compilation.moduleGraph.getOutgoingConnections(module).size,
      0
    )
  };
}

function normalizeModuleIdentifier(module: { identifier(): string }): string {
  return module.identifier().replaceAll("\\", "/");
}

function snapshotWebpackStats(stats: WebpackStats): StatsSnapshot {
  const json = stats.toJson({
    all: false,
    assets: true,
    errors: true,
    outputPath: true
  });
  return normalizeStatsSnapshot(stats.hasErrors(), json);
}

function snapshotUnpackStats(stats: UnpackStats): StatsSnapshot {
  return normalizeStatsSnapshot(stats.hasErrors(), stats.toJson());
}

function normalizeStatsSnapshot(
  hasErrors: boolean,
  json: {
    assets?: readonly { name?: string; size?: number }[];
    errors?: readonly unknown[];
    outputPath?: string;
  }
): StatsSnapshot {
  return {
    hasErrors,
    errorCount: json.errors?.length ?? 0,
    assets: (json.assets ?? [])
      .map(({ name, size }) => ({ name, size }))
      .sort((left, right) => (left.name ?? "").localeCompare(right.name ?? "")),
    outputPath: json.outputPath
  };
}

function assertCompilationLifetime(
  observation: CompilationLifetimeObservation,
  oldModuleSuffix: string,
  newModuleSuffix: string
): void {
  assert.equal(observation.distinctCompilations, true);
  assert.equal(observation.distinctModuleGraphs, true);
  assert.equal(observation.firstStatsReferencesFirstCompilation, true);
  assert.equal(observation.secondStatsReferencesSecondCompilation, true);
  assert.deepEqual(
    observation.oldCompilationDuringSecond,
    observation.oldCompilationBeforeSecond
  );
  assert.deepEqual(
    observation.oldCompilationAfterSecond,
    observation.oldCompilationBeforeSecond
  );
  assert.deepEqual(observation.oldStatsDuringSecond, observation.oldStatsBeforeSecond);
  assert.deepEqual(observation.oldStatsAfterSecond, observation.oldStatsBeforeSecond);
  assert.equal(
    observation.oldCompilationAfterSecond.moduleIdentifiers.some((identifier) =>
      identifier.endsWith(oldModuleSuffix)
    ),
    true
  );
  assert.equal(
    observation.oldCompilationAfterSecond.moduleIdentifiers.some((identifier) =>
      identifier.endsWith(newModuleSuffix)
    ),
    false
  );
  assert.equal(
    observation.newCompilationAfterSecond.moduleIdentifiers.some((identifier) =>
      identifier.endsWith(newModuleSuffix)
    ),
    true
  );
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

type ObservedRunCallback = (
  err: Error | null | undefined,
  stats: WebpackStats | UnpackStats | undefined
) => void;

type RunInvoker = (callback: ObservedRunCallback) => void;

async function observeConcurrentRuns(
  invoke: RunInvoker,
  invokeConflict: RunInvoker = invoke
) {
  let firstCalledSynchronously = true;
  let conflictCalledSynchronously = true;
  let firstCalls = 0;
  let conflictCalls = 0;
  const first = new Promise<RunObservation>((resolve) => {
    invoke((err, stats) => {
      firstCalls += 1;
      if (firstCalls === 1) {
        resolve(runObservation(firstCalledSynchronously, err, stats));
      }
    });
    firstCalledSynchronously = false;
  });
  const conflict = new Promise<RunObservation>((resolve) => {
    invokeConflict((err, stats) => {
      conflictCalls += 1;
      if (conflictCalls === 1) {
        resolve(runObservation(conflictCalledSynchronously, err, stats));
      }
    });
    conflictCalledSynchronously = false;
  });
  const [firstObservation, conflictObservation] = await Promise.all([
    first,
    conflict
  ]);
  await delay(0);
  return {
    first: firstObservation,
    conflict: conflictObservation,
    firstCalls,
    conflictCalls
  };
}

async function observeRunReentry(invoke: RunInvoker) {
  let firstCalledSynchronously = true;
  let firstCalls = 0;
  let secondCalls = 0;
  const result = await new Promise<{
    first: RunObservation;
    second: RunObservation;
  }>((resolve) => {
    invoke((firstError, firstStats) => {
      firstCalls += 1;
      const first = runObservation(
        firstCalledSynchronously,
        firstError,
        firstStats
      );
      let secondCalledSynchronously = true;
      invoke((secondError, secondStats) => {
        secondCalls += 1;
        if (secondCalls === 1) {
          resolve({
            first,
            second: runObservation(
              secondCalledSynchronously,
              secondError,
              secondStats
            )
          });
        }
      });
      secondCalledSynchronously = false;
    });
    firstCalledSynchronously = false;
  });
  await delay(0);
  return { ...result, firstCalls, secondCalls };
}

function assertRunReentry(observation: Awaited<ReturnType<typeof observeRunReentry>>) {
  assert.equal(observation.first.calledSynchronously, false);
  assert.equal(observation.first.err, null);
  assert.equal(observation.first.hasStats, true);
  assert.equal(observation.firstCalls, 1);
  assert.equal(observation.second.calledSynchronously, false);
  assert.equal(observation.second.err, null);
  assert.equal(observation.second.hasStats, true);
  assert.equal(observation.secondCalls, 1);
}

async function observeSingleRun(invoke: RunInvoker) {
  let calledSynchronously = true;
  let calls = 0;
  const result = await new Promise<RunObservation>((resolve) => {
    invoke((err, stats) => {
      calls += 1;
      if (calls === 1) {
        resolve(runObservation(calledSynchronously, err, stats));
      }
    });
    calledSynchronously = false;
  });
  await delay(0);
  return { ...result, calls };
}

async function observeClose(
  invoke: (callback: (err?: Error | null) => void) => void
) {
  let calledSynchronously = true;
  let calls = 0;
  const result = await new Promise<{
    calledSynchronously: boolean;
    err: Error | null;
  }>((resolve) => {
    invoke((err) => {
      calls += 1;
      if (calls === 1) {
        resolve({
          calledSynchronously,
          err: err ?? null
        });
      }
    });
    calledSynchronously = false;
  });
  await delay(0);
  return { ...result, calls };
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
