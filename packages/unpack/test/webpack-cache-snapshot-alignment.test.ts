import { utimes, writeFile } from "node:fs/promises";
import { join } from "node:path";
import assert from "node:assert/strict";
import test from "node:test";

import webpack from "webpack";
import type {
  Compiler as WebpackCompiler,
  Configuration as WebpackOptions
} from "webpack";

import unpack from "@unpack-js/core";
import type {
  Mode,
  SnapshotOptions,
  SnapshotStrategyOptions,
  UnpackOptions
} from "@unpack-js/core";

import {
  captureSynchronousThrow,
  closeUnpackCompiler,
  closeWebpackCompiler,
  createComparisonFixture,
  runNodeScript,
  runUnpackCompiler,
  runWebpack,
  runWebpackCompiler,
  unpackOptions,
  webpackNodeOptions,
  type ComparisonFixture,
  type UnpackBuildObservation,
  type WebpackBuildObservation
} from "./webpack-comparison-helpers.js";

const stableTime = new Date("2020-01-01T00:00:00.000Z");

test("cache false rebuilds changed sources instead of stale output", async () => {
  await assertSameTimestampModuleEdit({
    webpack: { cache: false },
    unpack: { cache: false },
    expectedValue: "after"
  });
});

test("snapshot module hash invalidates same-timestamp source edits", async () => {
  await assertSameTimestampModuleEdit({
    webpack: {
      cache: true,
      snapshot: {
        module: { timestamp: false, hash: true }
      }
    },
    unpack: {
      cache: true,
      snapshot: {
        module: { timestamp: false, hash: true }
      }
    },
    expectedValue: "after"
  });
});

test("missing resolver candidates invalidate after the missing file appears", async () => {
  const fixture = await createComparisonFixture("cache-snapshot-missing-candidate-", {
    "src/index.js": "import { value } from './missing'; export const result = value;"
  });

  try {
    const webpackCompiler = webpack(
      webpackNodeOptions(fixture.webpackRoot, { cache: true })
    ) as WebpackCompiler;
    const unpackCompiler = unpack(unpackOptions(fixture.unpackRoot, { cache: true }));

    try {
      const firstWebpack = await runWebpackCompiler(webpackCompiler);
      const firstUnpack = await runUnpackCompiler(unpackCompiler);
      assertBuildErrored(firstWebpack);
      assertBuildErrored(firstUnpack);

      await Promise.all([
        writeFile(join(fixture.webpackRoot, "src/missing.js"), "export const value = 'present';"),
        writeFile(join(fixture.unpackRoot, "src/missing.js"), "export const value = 'present';")
      ]);

      const secondWebpack = await runWebpackCompiler(webpackCompiler);
      const secondUnpack = await runUnpackCompiler(unpackCompiler);
      assertBuildSucceeded(secondWebpack);
      assertBuildSucceeded(secondUnpack);
      await assertRuntimeExport(fixture.webpackRoot, "result", "present");
      await assertRuntimeExport(fixture.unpackRoot, "result", "present");
    } finally {
      await closeWebpackCompiler(webpackCompiler);
      await closeUnpackCompiler(unpackCompiler);
    }
  } finally {
    await fixture.cleanup();
  }
});

test("observes mode-aware module snapshot default behavior", async () => {
  const cases: Array<{
    mode: Mode | undefined;
    expectedWebpack: string;
    expectedUnpack: string;
  }> = [
    { mode: undefined, expectedWebpack: "before", expectedUnpack: "after" },
    { mode: "production", expectedWebpack: "before", expectedUnpack: "after" },
    { mode: "development", expectedWebpack: "before", expectedUnpack: "before" },
    { mode: "none", expectedWebpack: "before", expectedUnpack: "before" }
  ];

  for (const { mode, expectedWebpack, expectedUnpack } of cases) {
    assert.deepEqual(await observeModuleDefault(mode), {
      webpack: expectedWebpack,
      unpack: expectedUnpack
    });
  }
});

test("observes mode-aware resolve snapshot default behavior", async () => {
  const cases: Array<{
    mode: Mode | undefined;
    expectedWebpack: string;
    expectedUnpack: string;
  }> = [
    { mode: undefined, expectedWebpack: "before", expectedUnpack: "after" },
    { mode: "production", expectedWebpack: "before", expectedUnpack: "after" },
    { mode: "development", expectedWebpack: "before", expectedUnpack: "before" },
    { mode: "none", expectedWebpack: "before", expectedUnpack: "before" }
  ];

  for (const { mode, expectedWebpack, expectedUnpack } of cases) {
    assert.deepEqual(await observeResolveDefault(mode), {
      webpack: expectedWebpack,
      unpack: expectedUnpack
    });
  }
});

test("observes unsupported snapshot option and unvalidated strategy boundaries", async () => {
  await assertWebpackAcceptsUnpackRejectsSnapshot({
    snapshot: { contextModule: { timestamp: true, hash: false } },
    expectedUnpackErrorName: "TypeError"
  });

  for (const strategyName of [
    "module",
    "resolve",
    "buildDependencies",
    "resolveBuildDependencies"
  ] as const) {
    await assertWebpackAcceptsUnpackRejectsSnapshot({
      snapshot: disabledStrategySnapshot(strategyName),
      expectedUnpackErrorName: "TypeError"
    });
  }
});

async function assertSameTimestampModuleEdit(options: {
  webpack: WebpackOptions;
  unpack: Partial<UnpackOptions>;
  expectedValue: string;
}) {
  const fixture = await createComparisonFixture("cache-snapshot-module-edit-", {
    "src/index.js": "export const value = 'before';"
  });

  try {
    await touchEntries(fixture);
    const webpackCompiler = webpack(
      webpackNodeOptions(fixture.webpackRoot, options.webpack)
    ) as WebpackCompiler;
    const unpackCompiler = unpack(unpackOptions(fixture.unpackRoot, options.unpack));

    try {
      assertBuildSucceeded(await runWebpackCompiler(webpackCompiler));
      assertBuildSucceeded(await runUnpackCompiler(unpackCompiler));
      await assertRuntimeExport(fixture.webpackRoot, "value", "before");
      await assertRuntimeExport(fixture.unpackRoot, "value", "before");

      await Promise.all([
        writeModuleValue(fixture.webpackRoot, "after"),
        writeModuleValue(fixture.unpackRoot, "after")
      ]);

      assertBuildSucceeded(await runWebpackCompiler(webpackCompiler));
      assertBuildSucceeded(await runUnpackCompiler(unpackCompiler));
      await assertRuntimeExport(fixture.webpackRoot, "value", options.expectedValue);
      await assertRuntimeExport(fixture.unpackRoot, "value", options.expectedValue);
    } finally {
      await closeWebpackCompiler(webpackCompiler);
      await closeUnpackCompiler(unpackCompiler);
    }
  } finally {
    await fixture.cleanup();
  }
}

async function observeModuleDefault(mode: Mode | undefined) {
  const fixture = await createComparisonFixture("cache-snapshot-module-default-", {
    "src/index.js": "export const value = 'before';"
  });

  try {
    await touchEntries(fixture);
    const webpackOptionsForMode = webpackNodeOptions(fixture.webpackRoot, {
      cache: true,
      ...(mode === undefined ? {} : { mode })
    });
    const unpackOptionsForMode = unpackOptions(fixture.unpackRoot, {
      cache: true,
      ...(mode === undefined ? {} : { mode })
    });
    omitModeWhenUndefined(webpackOptionsForMode, unpackOptionsForMode, mode);

    const webpackCompiler = webpack(webpackOptionsForMode) as WebpackCompiler;
    const unpackCompiler = unpack(unpackOptionsForMode);

    try {
      assertBuildSucceeded(await runWebpackCompiler(webpackCompiler));
      assertBuildSucceeded(await runUnpackCompiler(unpackCompiler));
      await Promise.all([
        writeModuleValue(fixture.webpackRoot, "after"),
        writeModuleValue(fixture.unpackRoot, "after")
      ]);

      assertBuildSucceeded(await runWebpackCompiler(webpackCompiler));
      assertBuildSucceeded(await runUnpackCompiler(unpackCompiler));

      return {
        webpack: await runtimeExport(fixture.webpackRoot, "value"),
        unpack: await runtimeExport(fixture.unpackRoot, "value")
      };
    } finally {
      await closeWebpackCompiler(webpackCompiler);
      await closeUnpackCompiler(unpackCompiler);
    }
  } finally {
    await fixture.cleanup();
  }
}

async function observeResolveDefault(mode: Mode | undefined) {
  const fixture = await createComparisonFixture("cache-snapshot-resolve-default-", {
    "src/index.js": "import { value } from 'pkg/feature'; export const result = value;",
    "node_modules/pkg/package.json": packageJsonWithFeature("./before.js"),
    "node_modules/pkg/before.js": "export const value = 'before';",
    "node_modules/pkg/after.js": "export const value = 'after';"
  });

  try {
    await touchPackageJsons(fixture);
    const webpackPackageRoot = join(fixture.webpackRoot, "node_modules/pkg");
    const unpackPackageRoot = join(fixture.unpackRoot, "node_modules/pkg");
    const webpackOptionsForMode = webpackNodeOptions(fixture.webpackRoot, {
      cache: true,
      snapshot: { unmanagedPaths: [webpackPackageRoot] },
      ...(mode === undefined ? {} : { mode })
    });
    const unpackOptionsForMode = unpackOptions(fixture.unpackRoot, {
      cache: true,
      snapshot: { unmanagedPaths: [unpackPackageRoot] },
      ...(mode === undefined ? {} : { mode })
    });
    omitModeWhenUndefined(webpackOptionsForMode, unpackOptionsForMode, mode);

    const webpackCompiler = webpack(webpackOptionsForMode) as WebpackCompiler;
    const unpackCompiler = unpack(unpackOptionsForMode);

    try {
      assertBuildSucceeded(await runWebpackCompiler(webpackCompiler));
      assertBuildSucceeded(await runUnpackCompiler(unpackCompiler));

      await Promise.all([
        writePackageExport(fixture.webpackRoot, "./after.js"),
        writePackageExport(fixture.unpackRoot, "./after.js")
      ]);

      assertBuildSucceeded(await runWebpackCompiler(webpackCompiler));
      assertBuildSucceeded(await runUnpackCompiler(unpackCompiler));

      return {
        webpack: await runtimeExport(fixture.webpackRoot, "result"),
        unpack: await runtimeExport(fixture.unpackRoot, "result")
      };
    } finally {
      await closeWebpackCompiler(webpackCompiler);
      await closeUnpackCompiler(unpackCompiler);
    }
  } finally {
    await fixture.cleanup();
  }
}

async function assertWebpackAcceptsUnpackRejectsSnapshot(options: {
  snapshot: Record<string, unknown>;
  expectedUnpackErrorName: string;
}) {
  const fixture = await createComparisonFixture("cache-snapshot-boundary-", {
    "src/index.js": "export const value = 1;"
  });

  try {
    assertBuildSucceeded(
      await runWebpack(
        webpackNodeOptions(fixture.webpackRoot, {
          snapshot: options.snapshot as WebpackOptions["snapshot"]
        })
      )
    );

    const unpackError = captureSynchronousThrow(() =>
      unpack(
        unpackOptions(fixture.unpackRoot, {
          snapshot: options.snapshot as SnapshotOptions
        })
      )
    );

    assert.equal(unpackError?.name, options.expectedUnpackErrorName);
  } finally {
    await fixture.cleanup();
  }
}

function disabledStrategySnapshot(strategyName: SnapshotStrategyName): Partial<
  Record<SnapshotStrategyName, SnapshotStrategyOptions>
> {
  return {
    [strategyName]: {
      timestamp: false,
      hash: false
    }
  };
}

async function touchEntries(fixture: ComparisonFixture) {
  await Promise.all([
    utimes(join(fixture.webpackRoot, "src/index.js"), stableTime, stableTime),
    utimes(join(fixture.unpackRoot, "src/index.js"), stableTime, stableTime)
  ]);
}

async function touchPackageJsons(fixture: ComparisonFixture) {
  await Promise.all([
    utimes(join(fixture.webpackRoot, "node_modules/pkg/package.json"), stableTime, stableTime),
    utimes(join(fixture.unpackRoot, "node_modules/pkg/package.json"), stableTime, stableTime)
  ]);
}

async function writeModuleValue(root: string, value: string) {
  await writeFile(join(root, "src/index.js"), `export const value = '${value}';`, {
    encoding: "utf8"
  });
  await utimes(join(root, "src/index.js"), stableTime, stableTime);
}

async function writePackageExport(root: string, target: string) {
  await writeFile(join(root, "node_modules/pkg/package.json"), packageJsonWithFeature(target), {
    encoding: "utf8"
  });
  await utimes(join(root, "node_modules/pkg/package.json"), stableTime, stableTime);
}

function packageJsonWithFeature(target: string) {
  return JSON.stringify({
    name: "pkg",
    version: "1.0.0",
    exports: {
      "./feature": target
    }
  });
}

function assertBuildSucceeded(observation: WebpackBuildObservation | UnpackBuildObservation) {
  assert.equal(observation.err, null);
  assert.equal(observation.hasStats, true);
  assert.equal(observation.hasErrors, false);
}

function assertBuildErrored(observation: WebpackBuildObservation | UnpackBuildObservation) {
  assert.equal(observation.err, null);
  assert.equal(observation.hasStats, true);
  assert.equal(observation.hasErrors, true);
}

async function assertRuntimeExport(root: string, name: string, expected: string) {
  assert.equal(await runtimeExport(root, name), expected);
}

async function runtimeExport(root: string, name: string) {
  const result = await runNodeScript(
    root,
    `const bundle = require("./main.js"); console.log(bundle[${JSON.stringify(name)}]);`
  );

  assert.equal(result.error, undefined);
  assert.equal(result.status, 0);
  assert.equal(result.signal, null);
  assert.equal(result.stderr, "");
  return result.stdout.trim();
}

function omitModeWhenUndefined(
  webpackOptionsForMode: WebpackOptions,
  unpackOptionsForMode: UnpackOptions,
  mode: Mode | undefined
) {
  if (mode === undefined) {
    delete webpackOptionsForMode.mode;
    delete unpackOptionsForMode.mode;
  }
}

type SnapshotStrategyName =
  | "module"
  | "resolve"
  | "buildDependencies"
  | "resolveBuildDependencies";
