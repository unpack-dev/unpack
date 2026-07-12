import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join } from "node:path";

import webpack from "webpack";
import type { ConfigCaseTest } from "../../../config-case.js";
import { createAnalyzerPlugin } from "./webpack.config.js";

interface AnalyzerStats {
  errors: unknown[];
  warnings: unknown[];
  assets: Array<{ name: string; size: number }>;
}

export default {
  async validate({ fixturePath, outputFiles, outputPath }) {
    assert.equal(outputFiles.includes("main.js"), true);
    assert.equal(outputFiles.includes("stats.json"), true);

    const unpackStats = await readAnalyzerStats(outputPath);
    const webpackStats = await runWebpackAnalyzer(fixturePath);

    assert.equal(unpackStats.assets.length, webpackStats.assets.length);
    assert.equal(unpackStats.assets.length, 2);
    assert.equal(unpackStats.assets.some((asset) => asset.name === "main.js"), true);
    assert.equal(webpackStats.assets.some((asset) => asset.name === "main.js"), true);
    assert.equal(unpackStats.assets.every((asset) => asset.size > 0), true);
    assert.equal(webpackStats.assets.every((asset) => asset.size > 0), true);
    assert.deepEqual(unpackStats.errors, []);
    assert.deepEqual(unpackStats.warnings, []);
  }
} satisfies ConfigCaseTest;

async function runWebpackAnalyzer(fixturePath: string): Promise<AnalyzerStats> {
  const outputPath = join(fixturePath, "dist-webpack");
  const compiler = webpack({
    context: fixturePath,
    mode: "none",
    entry: "./index.js",
    output: { path: outputPath },
    devtool: false,
    optimization: {
      concatenateModules: false,
      innerGraph: false,
      minimize: false,
      providedExports: false,
      sideEffects: false,
      usedExports: false
    },
    plugins: [
      createAnalyzerPlugin() as unknown as import("webpack").WebpackPluginInstance
    ]
  });

  try {
    await new Promise<void>((resolve, reject) => {
      compiler.run((error) => (error ? reject(error) : resolve()));
    });
    return readAnalyzerStats(outputPath);
  } finally {
    await new Promise<void>((resolve, reject) => {
      compiler.close((error) => (error ? reject(error) : resolve()));
    });
  }
}

async function readAnalyzerStats(outputPath: string): Promise<AnalyzerStats> {
  return JSON.parse(
    await readFile(join(outputPath, "stats.json"), "utf8")
  ) as AnalyzerStats;
}
