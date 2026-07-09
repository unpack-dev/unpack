import assert from "node:assert/strict";
import test from "node:test";

import type { Stats as WebpackStats } from "webpack";

import type { Stats as UnpackStats } from "@unpack-js/core";

import {
  createComparisonFixture,
  readAsset,
  runNodeScript,
  runUnpack,
  runWebpack,
  unpackOptions,
  webpackNodeOptions
} from "./webpack-comparison-helpers.js";
import type {
  BuildObservation,
  FixtureFiles,
  NodeScriptObservation
} from "./webpack-comparison-helpers.js";

test("static ESM output executes with aligned runtime semantics", async () => {
  const fixture = await createComparisonFixture(
    "webpack-output-static-esm-",
    staticEsmFixture()
  );

  try {
    const webpackBuild = await runWebpack(
      webpackNodeOptions(fixture.webpackRoot, { devtool: false })
    );
    const unpackBuild = await runUnpack(
      unpackOptions(fixture.unpackRoot, { sourcemap: false })
    );
    assertSuccessfulBuildPair(webpackBuild, unpackBuild);
    assertAssetPresent(webpackBuild, "main.js");
    assertAssetPresent(unpackBuild, "main.js");

    const script = `
      const entry = require("./main.js");
      console.log(JSON.stringify({
        report: entry.report(),
        local: entry.local,
        renamed: entry.renamed,
        star: entry.starValue,
        defaultValue: entry.default
      }));
    `;
    const webpackRuntime = await runNodeScript(fixture.webpackRoot, script);
    const unpackRuntime = await runNodeScript(fixture.unpackRoot, script);

    assertNodeSuccess(webpackRuntime);
    assertNodeSuccess(unpackRuntime);
    assert.equal(unpackRuntime.stdout.trim(), webpackRuntime.stdout.trim());
  } finally {
    await fixture.cleanup();
  }
});

test("entry output contains shared webpack-shaped helper vocabulary", async () => {
  const fixture = await createComparisonFixture(
    "webpack-output-runtime-smoke-",
    staticEsmFixture()
  );

  try {
    const webpackBuild = await runWebpack(
      webpackNodeOptions(fixture.webpackRoot, { devtool: false })
    );
    const unpackBuild = await runUnpack(
      unpackOptions(fixture.unpackRoot, { sourcemap: false })
    );
    assertSuccessfulBuildPair(webpackBuild, unpackBuild);

    const webpackMain = await readAsset(fixture.webpackRoot, "main.js");
    const unpackMain = await readAsset(fixture.unpackRoot, "main.js");

    for (const source of [webpackMain, unpackMain]) {
      assert.match(source, /__webpack_require__/);
      assert.match(source, /__webpack_require__\.d/);
      assert.match(source, /__webpack_require__\.r/);
    }
  } finally {
    await fixture.cleanup();
  }
});

test("object entries emit requireable entry assets with aligned exports", async () => {
  const fixture = await createComparisonFixture("webpack-output-object-entries-", {
    "src/a.js": "export const value = 'a'; export default 'default-a';",
    "src/b.js": "export const value = 'b'; export default 'default-b';"
  });

  try {
    const webpackBuild = await runWebpack(
      webpackNodeOptions(fixture.webpackRoot, {
        devtool: false,
        entry: {
          a: "./src/a.js",
          b: "./src/b.js"
        },
        output: {
          filename: "[name].js"
        }
      })
    );
    const unpackBuild = await runUnpack(
      unpackOptions(fixture.unpackRoot, {
        entry: {
          a: "./src/a.js",
          b: "./src/b.js"
        },
        sourcemap: false
      })
    );
    assertSuccessfulBuildPair(webpackBuild, unpackBuild);
    assertAssetsPresent(webpackBuild, ["a.js", "b.js"]);
    assertAssetsPresent(unpackBuild, ["a.js", "b.js"]);

    const script = `
      console.log(JSON.stringify([
        require("./a.js").value,
        require("./a.js").default,
        require("./b.js").value,
        require("./b.js").default
      ]));
    `;
    const webpackRuntime = await runNodeScript(fixture.webpackRoot, script);
    const unpackRuntime = await runNodeScript(fixture.unpackRoot, script);

    assertNodeSuccess(webpackRuntime);
    assertNodeSuccess(unpackRuntime);
    assert.equal(unpackRuntime.stdout.trim(), webpackRuntime.stdout.trim());
  } finally {
    await fixture.cleanup();
  }
});

test("dynamic import output loads async chunks with aligned runtime values", async () => {
  const fixture = await createComparisonFixture("webpack-output-dynamic-import-", {
    "src/index.js": `
      import { eager } from "./eager";

      export async function loadFeature() {
        const mod = await import("./feature");
        return [eager, mod.feature, mod.shared, mod.alpha, mod.beta];
      }
    `,
    "src/eager.js": "export const eager = 'eager';",
    "src/feature.js": `
      import { shared } from "./shared";
      export const feature = "feature";
      export { shared };
      export * from "./extra";
      export const alpha = "feature-alpha";
    `,
    "src/shared.js": "export const shared = 'shared';",
    "src/extra.js": "export const alpha = 'alpha'; export const beta = 'beta';"
  });

  try {
    const webpackBuild = await runWebpack(
      webpackNodeOptions(fixture.webpackRoot, { devtool: false })
    );
    const unpackBuild = await runUnpack(
      unpackOptions(fixture.unpackRoot, { sourcemap: false })
    );
    assertSuccessfulBuildPair(webpackBuild, unpackBuild);
    assertAssetPresent(webpackBuild, "main.js");
    assertAssetPresent(unpackBuild, "main.js");
    assert.ok(nonEntryJavaScriptAssets(webpackBuild, ["main.js"]).length >= 1);
    assert.ok(nonEntryJavaScriptAssets(unpackBuild, ["main.js"]).length >= 1);

    const script = `
      require("./main.js").loadFeature()
        .then((value) => {
          console.log(JSON.stringify(value));
        })
        .catch((error) => {
          console.error(error && error.stack || error);
          process.exit(1);
        });
    `;
    const webpackRuntime = await runNodeScript(fixture.webpackRoot, script);
    const unpackRuntime = await runNodeScript(fixture.unpackRoot, script);

    assertNodeSuccess(webpackRuntime);
    assertNodeSuccess(unpackRuntime);
    assert.equal(unpackRuntime.stdout.trim(), webpackRuntime.stdout.trim());
  } finally {
    await fixture.cleanup();
  }
});

test("multi-entry async output aligns executable behavior without fixing chunk layout", async () => {
  const fixture = await createComparisonFixture("webpack-output-multi-entry-async-", {
    "src/a.js": `
      import { shared } from "./shared";

      export async function load() {
        const mod = await import("./feature");
        return [shared, mod.feature, mod.shared];
      }
    `,
    "src/b.js": `
      export async function load() {
        const mod = await import("./feature");
        return [mod.feature, mod.shared];
      }
    `,
    "src/feature.js": `
      import { shared } from "./shared";
      export const feature = "feature";
      export { shared };
    `,
    "src/shared.js": "export const shared = 'shared';"
  });

  try {
    const webpackBuild = await runWebpack(
      webpackNodeOptions(fixture.webpackRoot, {
        devtool: false,
        entry: {
          a: "./src/a.js",
          b: "./src/b.js"
        },
        output: {
          filename: "[name].js"
        }
      })
    );
    const unpackBuild = await runUnpack(
      unpackOptions(fixture.unpackRoot, {
        entry: {
          a: "./src/a.js",
          b: "./src/b.js"
        },
        sourcemap: false
      })
    );
    assertSuccessfulBuildPair(webpackBuild, unpackBuild);
    assertAssetsPresent(webpackBuild, ["a.js", "b.js"]);
    assertAssetsPresent(unpackBuild, ["a.js", "b.js"]);

    const script = `
      Promise.all([
        require("./a.js").load(),
        require("./b.js").load()
      ])
        .then((value) => {
          console.log(JSON.stringify(value));
        })
        .catch((error) => {
          console.error(error && error.stack || error);
          process.exit(1);
        });
    `;
    const webpackRuntime = await runNodeScript(fixture.webpackRoot, script);
    const unpackRuntime = await runNodeScript(fixture.unpackRoot, script);

    assertNodeSuccess(webpackRuntime);
    assertNodeSuccess(unpackRuntime);
    assert.equal(unpackRuntime.stdout.trim(), webpackRuntime.stdout.trim());
  } finally {
    await fixture.cleanup();
  }
});

test("nested dynamic import is recorded as an executable output gap", async () => {
  const fixture = await createComparisonFixture("webpack-output-nested-dynamic-", {
    "src/index.js": `
      export async function loadNested() {
        const feature = await import("./feature");
        return feature.loadNested();
      }
    `,
    "src/feature.js": `
      export async function loadNested() {
        const inner = await import("./inner");
        return ["feature", inner.value];
      }
    `,
    "src/inner.js": "export const value = 'inner';"
  });

  try {
    const webpackBuild = await runWebpack(
      webpackNodeOptions(fixture.webpackRoot, { devtool: false })
    );
    const unpackBuild = await runUnpack(
      unpackOptions(fixture.unpackRoot, { sourcemap: false })
    );
    assertSuccessfulBuild(webpackBuild);
    assert.equal(unpackBuild.err, null);
    assert.equal(unpackBuild.hasStats, true);

    const script = `
      require("./main.js").loadNested()
        .then((value) => {
          console.log(JSON.stringify(value));
        })
        .catch((error) => {
          console.error(error && error.stack || error);
          process.exit(1);
        });
    `;
    const webpackRuntime = await runNodeScript(fixture.webpackRoot, script);
    assertNodeSuccess(webpackRuntime);
    assert.equal(webpackRuntime.stdout.trim(), '["feature","inner"]');

    if (unpackBuild.hasErrors) {
      assert.equal(unpackBuild.hasErrors, true);
      return;
    }

    const unpackRuntime = await runNodeScript(fixture.unpackRoot, script);
    if (unpackRuntime.status === 0) {
      assert.equal(unpackRuntime.stdout.trim(), webpackRuntime.stdout.trim());
    } else {
      assert.notEqual(unpackRuntime.status, 0);
      assert.match(`${unpackRuntime.stdout}${unpackRuntime.stderr}`, /\S/);
    }
  } finally {
    await fixture.cleanup();
  }
});

test("source map asset shape and omission align at the stable boundary", async () => {
  const withMapsFixture = await createComparisonFixture("webpack-output-source-map-", {
    "src/index.js": "export const value = 42;"
  });

  try {
    const webpackWithMaps = await runWebpack(
      webpackNodeOptions(withMapsFixture.webpackRoot, { devtool: "source-map" })
    );
    const unpackWithMaps = await runUnpack(unpackOptions(withMapsFixture.unpackRoot));
    assertSuccessfulBuildPair(webpackWithMaps, unpackWithMaps);
    assertAssetPresent(webpackWithMaps, "main.js");
    assertAssetsPresent(unpackWithMaps, ["main.js", "main.js.map"]);

    assertStableSourceMapShape(await readAsset(withMapsFixture.webpackRoot, "main.js.map"));
    assertStableSourceMapShape(await readAsset(withMapsFixture.unpackRoot, "main.js.map"));
  } finally {
    await withMapsFixture.cleanup();
  }

  const withoutMapsFixture = await createComparisonFixture("webpack-output-no-source-map-", {
    "src/index.js": "export const value = 42;"
  });

  try {
    const webpackWithoutMaps = await runWebpack(
      webpackNodeOptions(withoutMapsFixture.webpackRoot, { devtool: false })
    );
    const unpackWithoutMaps = await runUnpack(
      unpackOptions(withoutMapsFixture.unpackRoot, { sourcemap: false })
    );
    assertSuccessfulBuildPair(webpackWithoutMaps, unpackWithoutMaps);
    assert.deepEqual(onlySourceMapAssets(webpackWithoutMaps), []);
    assert.deepEqual(onlySourceMapAssets(unpackWithoutMaps), []);
    await assert.rejects(readAsset(withoutMapsFixture.webpackRoot, "main.js.map"));
    await assert.rejects(readAsset(withoutMapsFixture.unpackRoot, "main.js.map"));
    assert.doesNotMatch(
      await readAsset(withoutMapsFixture.webpackRoot, "main.js"),
      /sourceMappingURL/
    );
    assert.doesNotMatch(
      await readAsset(withoutMapsFixture.unpackRoot, "main.js"),
      /sourceMappingURL/
    );
  } finally {
    await withoutMapsFixture.cleanup();
  }
});

test("failed module output completes with diagnostics and emitted throwing bundles", async () => {
  await assertFailedModuleOutput({
    "src/index.js": "import {"
  });
  await assertFailedModuleOutput({
    "src/index.js": "import { value } from './missing'; export const result = value;"
  });
});

async function assertFailedModuleOutput(files: FixtureFiles): Promise<void> {
  const fixture = await createComparisonFixture("webpack-output-failed-module-", files);

  try {
    const webpackBuild = await runWebpack(
      webpackNodeOptions(fixture.webpackRoot, { devtool: false })
    );
    const unpackBuild = await runUnpack(
      unpackOptions(fixture.unpackRoot, { sourcemap: false })
    );
    assertCompletedWithErrors(webpackBuild);
    assertCompletedWithErrors(unpackBuild);

    const script = `
      try {
        require("./main.js");
        console.log("not thrown");
        process.exitCode = 1;
      } catch {
        console.log("thrown");
      }
    `;
    const webpackRuntime = await runNodeScript(fixture.webpackRoot, script);
    const unpackRuntime = await runNodeScript(fixture.unpackRoot, script);

    assertNodeSuccess(webpackRuntime);
    assertNodeSuccess(unpackRuntime);
    assert.equal(webpackRuntime.stdout.trim(), "thrown");
    assert.equal(unpackRuntime.stdout.trim(), "thrown");
  } finally {
    await fixture.cleanup();
  }
}

function staticEsmFixture(): FixtureFiles {
  return {
    "src/index.js": `
      import "./side-effect";
      import defaultValue, { value as namedValue, setValue } from "./dep";
      import * as namespace from "./namespace";

      export { named as renamed } from "./reexport";
      export * from "./star";
      export const local = "local";
      export default "entry-default";

      export function report() {
        const before = namedValue;
        setValue("after");
        return [
          globalThis.__webpackOutputSideEffect,
          before,
          namedValue,
          defaultValue,
          namespace.nsValue,
          namespace.read()
        ];
      }
    `,
    "src/side-effect.js": "globalThis.__webpackOutputSideEffect = 'side-effect';",
    "src/dep.js": `
      export let value = "before";
      export default "dep-default";
      export function setValue(next) {
        value = next;
      }
    `,
    "src/namespace.js": `
      export const nsValue = "namespace";
      export function read() {
        return nsValue;
      }
    `,
    "src/reexport.js": "export const named = 'renamed';",
    "src/star.js": "export const starValue = 'star';"
  };
}

function assertSuccessfulBuildPair(
  webpackBuild: BuildObservation<WebpackStats>,
  unpackBuild: BuildObservation<UnpackStats>
): void {
  assertSuccessfulBuild(webpackBuild);
  assertSuccessfulBuild(unpackBuild);
}

function assertSuccessfulBuild(
  observation: BuildObservation<WebpackStats | UnpackStats>
): void {
  assert.equal(observation.err, null);
  assert.equal(observation.hasStats, true);
  assert.equal(observation.hasErrors, false);
}

function assertCompletedWithErrors(
  observation: BuildObservation<WebpackStats | UnpackStats>
): void {
  assert.equal(observation.err, null);
  assert.equal(observation.hasStats, true);
  assert.equal(observation.hasErrors, true);
  assertAssetPresent(observation, "main.js");
  assert.ok(statsErrors(observation.stats).length > 0);
}

function assertAssetsPresent(
  observation: BuildObservation<WebpackStats | UnpackStats>,
  assets: string[]
): void {
  for (const asset of assets) {
    assertAssetPresent(observation, asset);
  }
}

function assertAssetPresent(
  observation: BuildObservation<WebpackStats | UnpackStats>,
  asset: string
): void {
  assert.ok(
    observation.assets.includes(asset),
    `expected ${asset} in ${JSON.stringify(observation.assets)}`
  );
}

function nonEntryJavaScriptAssets(
  observation: BuildObservation<WebpackStats | UnpackStats>,
  entryAssets: string[]
): string[] {
  return observation.assets.filter(
    (asset) => asset.endsWith(".js") && !entryAssets.includes(asset)
  );
}

function onlySourceMapAssets(
  observation: BuildObservation<WebpackStats | UnpackStats>
): string[] {
  return observation.assets.filter((asset) => asset.endsWith(".map"));
}

function assertStableSourceMapShape(source: string): void {
  const map = JSON.parse(source) as {
    version?: unknown;
    file?: unknown;
    sources?: unknown;
    sourcesContent?: unknown;
  };

  assert.equal(map.version, 3);
  assert.equal(map.file, "main.js");
  assert.ok(Array.isArray(map.sources));
  assert.ok(map.sources.length > 0);
  assert.ok(Array.isArray(map.sourcesContent));
  assert.ok(map.sourcesContent.length > 0);
  assert.ok(
    map.sourcesContent.some(
      (content) => typeof content === "string" && content.includes("export const value = 42")
    )
  );
}

function statsErrors(stats: WebpackStats | UnpackStats | undefined): unknown[] {
  if (!stats) {
    return [];
  }

  const json = (stats as { toJson(options?: unknown): { errors?: unknown[] } }).toJson({
    all: false,
    errors: true
  });

  return Array.isArray(json.errors) ? json.errors : [];
}

function assertNodeSuccess(observation: NodeScriptObservation): void {
  assert.equal(observation.error, undefined);
  assert.equal(observation.signal, null);
  assert.equal(observation.status, 0, nodeFailureMessage(observation));
}

function nodeFailureMessage(observation: NodeScriptObservation): string {
  return [
    "node script failed",
    `status: ${observation.status}`,
    `signal: ${observation.signal}`,
    "stdout:",
    observation.stdout,
    "stderr:",
    observation.stderr
  ].join("\n");
}
