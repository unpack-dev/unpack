import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import unpack from "@unpack-js/core";
import type { Compiler as UnpackCompiler, WebpackPlugin } from "@unpack-js/core";
import webpack from "webpack";
import type { Compiler as WebpackCompiler, WebpackPluginInstance } from "webpack";
import webpackSources from "webpack-sources";

const { ConcatSource, RawSource } = webpackSources;

interface ProcessAssetsCompilation {
  readonly assets: Record<string, import("webpack-sources").Source>;
  readonly hooks: {
    processAssets: {
      tapPromise(
        name: string,
        callback: (assets: Record<string, import("webpack-sources").Source>) => Promise<void>
      ): void;
    };
  };
  emitAsset(name: string, source: import("webpack-sources").Source): void;
  updateAsset(name: string, source: import("webpack-sources").Source): void;
}

interface ProcessAssetsCompiler {
  readonly hooks: {
    thisCompilation: {
      tap(name: string, callback: (compilation: ProcessAssetsCompilation) => void): void;
    };
  };
}

test("processAssets matches webpack ordering and emitted asset mutations", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-process-assets-"));
  await mkdir(join(fixture, "src"), { recursive: true });
  await writeFile(join(fixture, "src/index.js"), "export default 42;\n");
  const unpackOutput = join(fixture, "dist-unpack");
  const webpackOutput = join(fixture, "dist-webpack");
  const unpackEvents: string[] = [];
  const webpackEvents: string[] = [];
  const unpackCompiler = unpack({
    context: fixture,
    mode: "none",
    entry: "./src/index.js",
    output: { path: unpackOutput },
    sourcemap: false,
    plugins: [createProcessAssetsPlugin(unpackEvents) as unknown as WebpackPlugin]
  });
  assert.ok(unpackCompiler);
  const webpackCompiler = webpack({
    context: fixture,
    mode: "none",
    entry: "./src/index.js",
    output: { path: webpackOutput },
    devtool: false,
    plugins: [
      createProcessAssetsPlugin(webpackEvents) as unknown as WebpackPluginInstance
    ]
  });

  try {
    await Promise.all([runUnpack(unpackCompiler), runWebpack(webpackCompiler)]);
    assert.deepEqual(unpackEvents, webpackEvents);
    assert.deepEqual(unpackEvents, ["first:start", "first:end", "second"]);
    for (const outputPath of [unpackOutput, webpackOutput]) {
      assert.equal(
        (await readFile(join(outputPath, "main.js"), "utf8")).startsWith("//banner;\n"),
        true
      );
      assert.equal(await readFile(join(outputPath, "early.txt"), "utf8"), "early");
      assert.equal(await readFile(join(outputPath, "manifest.txt"), "utf8"), "manifest");
    }
  } finally {
    await Promise.all([closeUnpack(unpackCompiler), closeWebpack(webpackCompiler)]);
    await rm(fixture, { recursive: true, force: true });
  }
});

function createProcessAssetsPlugin(events: string[]): { apply(compiler: ProcessAssetsCompiler): void } {
  return {
    apply(compiler) {
      compiler.hooks.thisCompilation.tap("process assets comparison", (compilation) => {
        compilation.emitAsset("early.txt", new RawSource("early"));
        compilation.hooks.processAssets.tapPromise("first", async () => {
          events.push("first:start");
          await Promise.resolve();
          compilation.updateAsset(
            "main.js",
            new ConcatSource(new RawSource("//banner;\n"), compilation.assets["main.js"])
          );
          compilation.emitAsset("manifest.txt", new RawSource("manifest"));
          events.push("first:end");
        });
        compilation.hooks.processAssets.tapPromise("second", async (assets) => {
          assert.deepEqual(Object.keys(assets).sort(), ["early.txt", "main.js", "manifest.txt"]);
          events.push("second");
        });
      });
    }
  };
}

function runUnpack(compiler: UnpackCompiler): Promise<void> {
  return new Promise((resolve, reject) => {
    compiler.run((error) => error ? reject(error) : resolve());
  });
}

function runWebpack(compiler: WebpackCompiler): Promise<void> {
  return new Promise((resolve, reject) => {
    compiler.run((error) => error ? reject(error) : resolve());
  });
}

function closeUnpack(compiler: UnpackCompiler): Promise<void> {
  return new Promise((resolve, reject) => {
    compiler.close((error) => error ? reject(error) : resolve());
  });
}

function closeWebpack(compiler: WebpackCompiler): Promise<void> {
  return new Promise((resolve, reject) => {
    compiler.close((error) => error ? reject(error) : resolve());
  });
}
