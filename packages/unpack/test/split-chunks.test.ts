import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile, mkdir } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

import unpack, { type Stats } from "@unpack-js/core";
import webpack, { type Configuration, type Stats as WebpackStats } from "webpack";

test("optimization.splitChunks extracts modules shared by async chunks", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-split-chunks-"));
  const outputPath = join(fixture, "dist");
  let sharedChunkGroupCount = 0;
  try {
    await writeFixture(fixture, {
      "src/index.js": "export default Promise.all([import('./a'), import('./b'), import('./c')]).then(([a, b, c]) => a.value + b.value + c.value);",
      "src/a.js": "import { sharedAB } from './shared-ab'; export const value = sharedAB + 1;",
      "src/b.js": "import { sharedAB } from './shared-ab'; import { sharedBC } from './shared-bc'; export const value = sharedAB + sharedBC;",
      "src/c.js": "import { sharedBC } from './shared-bc'; export const value = sharedBC + 2;",
      "src/shared-ab.js": `export const sharedAB = 20; // SHARED_AB_SPLIT_CHUNK_MARKER\n/* ${"x".repeat(25_000)} */`,
      "src/shared-bc.js": `export const sharedBC = 30; // SHARED_BC_SPLIT_CHUNK_MARKER\n/* ${"y".repeat(25_000)} */`
    });

    const webpackOutputPath = join(fixture, "webpack-dist");
    const webpackStats = await runWebpack({
      context: fixture,
      mode: "none",
      entry: "./src/index.js",
      output: { path: webpackOutputPath },
      devtool: false,
      optimization: { splitChunks: { chunks: "async", minChunks: 2, name: "shared" } }
    });
    assert.equal(webpackStats.hasErrors(), false);
    assert.ok(webpackStats.toJson({ assets: true }).assets?.some((asset) => asset.name === "shared.js"));

    const stats = await run({
      context: fixture,
      mode: "none",
      entry: "./src/index.js",
      output: { path: outputPath },
      sourcemap: false,
      optimization: { splitChunks: { chunks: "async", minChunks: 2, name: "shared" } },
      plugins: [{
        apply(compiler) {
          let compilation: Parameters<Parameters<typeof compiler.hooks.compilation.tap>[1]>[0];
          compiler.hooks.compilation.tap("ObserveSplitChunks", (current) => {
            compilation = current;
          });
          compiler.hooks.done.tap("ObserveSplitChunks", () => {
            sharedChunkGroupCount = compilation.chunkGroups.filter((group) =>
              group.chunks.some((chunk) => chunk.name === "shared")
            ).length;
          });
        }
      }]
    });
    assert.equal(stats.hasErrors(), false);
    const javascriptAssets = stats.toJson().assets
      .map((asset) => asset.name)
      .filter((name) => name.endsWith(".js"));
    assert.ok(javascriptAssets.includes("shared.js"));
    assert.equal(sharedChunkGroupCount, 3);
    const sources = await Promise.all(
      javascriptAssets.map((name) => readFile(join(outputPath, name), "utf8"))
    );
    assert.equal(sources.filter((source) => source.includes("SHARED_AB_SPLIT_CHUNK_MARKER")).length, 1);
    assert.equal(sources.filter((source) => source.includes("SHARED_BC_SPLIT_CHUNK_MARKER")).length, 1);

    const require = createRequire(import.meta.url);
    const entry = require(join(outputPath, "main.js")) as { default: Promise<number> };
    assert.equal(await entry.default, 103);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("optimization.splitChunks rejects unsupported surfaces synchronously", () => {
  assert.doesNotThrow(() => unpack({ entry: "./index.js", optimization: { splitChunks: false } }));
  assert.doesNotThrow(() => unpack({ entry: "./index.js", optimization: { splitChunks: {} } }));
  assert.throws(
    () => unpack({ entry: "./index.js", optimization: { splitChunks: { chunks: "all" as "async" } } }),
    /currently only supports 'async'/
  );
  assert.throws(
    () => unpack({ entry: "./index.js", optimization: { splitChunks: { minChunks: 0 } } }),
    /minChunks must be at least 1/
  );
  assert.throws(
    () => unpack({
      entry: "./index.js",
      optimization: { splitChunks: { cacheGroups: {} } as never }
    }),
    /unknown option 'cacheGroups'/
  );
  assert.throws(
    () => unpack({ entry: "./index.js", optimization: { splitChunks: { name: "" } } }),
    /name must not be empty/
  );
});

function run(options: Parameters<typeof unpack>[0]): Promise<Stats> {
  return new Promise((resolve, reject) => {
    unpack(options, (error, stats) => {
      if (error) reject(error);
      else if (stats) resolve(stats);
      else reject(new Error("compiler completed without Stats"));
    });
  });
}

function runWebpack(options: Configuration): Promise<WebpackStats> {
  return new Promise((resolve, reject) => {
    webpack(options, (error, stats) => {
      if (error) reject(error);
      else if (stats) resolve(stats);
      else reject(new Error("webpack completed without Stats"));
    });
  });
}

async function writeFixture(root: string, files: Record<string, string>): Promise<void> {
  for (const [name, source] of Object.entries(files)) {
    const path = join(root, name);
    await mkdir(dirname(path), { recursive: true });
    await writeFile(path, source);
  }
}
