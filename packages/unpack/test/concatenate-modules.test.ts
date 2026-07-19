import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

import unpack, { type Compiler, type Stats } from "@unpack-js/core";
import webpack, { type Compiler as WebpackCompiler, type Stats as WebpackStats } from "webpack";

test("optimization.concatenateModules rejects non-boolean values synchronously", () => {
  assert.throws(
    () => unpack({
      entry: "./src/index.js",
      optimization: { concatenateModules: "yes" as never }
    }),
    /options\.optimization\.concatenateModules must be a boolean/
  );
});

test("optimization.concatenateModules matches webpack chunk membership and runtime semantics", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-concatenate-modules-"));
  try {
    await writeFixture(fixture, {
      "src/index.js": [
        "import { increment, value } from './counter.js';",
        "increment();",
        "globalThis.CONCATENATE_MODULES_RESULT = value;",
        "export { value };"
      ].join("\n"),
      "src/counter.js": [
        "export let value = 40;",
        "export const increment = () => value += 2;"
      ].join("\n")
    });

    for (const concatenateModules of [false, true]) {
      const unpackOutputPath = join(fixture, `dist-unpack-${concatenateModules}`);
      const webpackOutputPath = join(fixture, `dist-webpack-${concatenateModules}`);
      const unpackObservation = await runUnpack(fixture, unpackOutputPath, concatenateModules);
      const webpackObservation = await runWebpack(fixture, webpackOutputPath, concatenateModules);

      assert.deepEqual(unpackObservation, webpackObservation);
      assert.deepEqual(unpackObservation, {
        chunkModuleCount: concatenateModules ? 1 : 2,
        runtimeValue: 42
      });
    }
  } finally {
    delete (globalThis as { CONCATENATE_MODULES_RESULT?: number }).CONCATENATE_MODULES_RESULT;
    await rm(fixture, { recursive: true, force: true });
  }
});

test("concatenated modules preserve cyclic live bindings through namespace reexports", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-concatenate-cycle-"));
  try {
    await writeFixture(fixture, {
      "src/index.js": [
        "import * as namespace from './barrel.js';",
        "globalThis.CONCATENATE_MODULES_RESULT = namespace.read();",
        "export default namespace.read;"
      ].join("\n"),
      "src/barrel.js": "export { read } from './a.js';",
      "src/a.js": [
        "import { step } from './b.js';",
        "export let value = 40;",
        "export const read = () => step(value);"
      ].join("\n"),
      "src/b.js": [
        "import { value } from './a.js';",
        "export const step = (input) => input + 2;",
        "export const readLiveValue = () => value;"
      ].join("\n")
    });

    const unpackObservation = await runUnpack(fixture, join(fixture, "dist-unpack"), true);
    const webpackObservation = await runWebpack(fixture, join(fixture, "dist-webpack"), true);
    assert.deepEqual(unpackObservation, webpackObservation);
    assert.deepEqual(unpackObservation, { chunkModuleCount: 1, runtimeValue: 42 });
  } finally {
    delete (globalThis as { CONCATENATE_MODULES_RESULT?: number }).CONCATENATE_MODULES_RESULT;
    await rm(fixture, { recursive: true, force: true });
  }
});

test("concatenated modules do not capture user bindings that resemble generated names", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-concatenate-bindings-"));
  try {
    await writeFixture(fixture, {
      "src/index.js": [
        "import { value } from './value.js';",
        "const __webpack_init__1 = 'user init';",
        "const __webpack_exports__1 = 'user exports';",
        "globalThis.CONCATENATE_MODULES_RESULT = `${value}:${__webpack_init__1}:${__webpack_exports__1}`;",
        "export { value };"
      ].join("\n"),
      "src/value.js": "export const value = 42;"
    });

    const unpackObservation = await runStringResultUnpack(
      fixture,
      join(fixture, "dist-unpack")
    );
    const webpackObservation = await runStringResultWebpack(
      fixture,
      join(fixture, "dist-webpack")
    );
    assert.equal(unpackObservation, webpackObservation);
    assert.equal(unpackObservation, "42:user init:user exports");
  } finally {
    delete (globalThis as { CONCATENATE_MODULES_RESULT?: string }).CONCATENATE_MODULES_RESULT;
    await rm(fixture, { recursive: true, force: true });
  }
});

test("concatenated modules retain each original source in sourcemaps", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-concatenate-sourcemap-"));
  try {
    await writeFixture(fixture, {
      "src/index.js": [
        "import { value } from './value.js';",
        "globalThis.CONCATENATE_MODULES_RESULT = value;",
        "export { value };"
      ].join("\n"),
      "src/value.js": "export const value = 42;"
    });
    const outputPath = join(fixture, "dist");
    await runUnpack(fixture, outputPath, true, true);
    const sourceMap = JSON.parse(await readFile(join(outputPath, "main.js.map"), "utf8")) as {
      sources: string[];
    };
    assert.ok(sourceMap.sources.some((source) => source.endsWith("src/index.js")));
    assert.ok(sourceMap.sources.some((source) => source.endsWith("src/value.js")));
  } finally {
    delete (globalThis as { CONCATENATE_MODULES_RESULT?: number }).CONCATENATE_MODULES_RESULT;
    await rm(fixture, { recursive: true, force: true });
  }
});

test("optimization.concatenateModules uses webpack mode defaults", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-concatenate-defaults-"));
  try {
    await writeFixture(fixture, {
      "src/index.js": [
        "import { value } from './value.js';",
        "globalThis.CONCATENATE_MODULES_RESULT = value;",
        "export { value };"
      ].join("\n"),
      "src/value.js": "export const value = 42;"
    });
    for (const [mode, expectedCount] of [
      ["production", 1],
      ["development", 2],
      ["none", 2]
    ] as const) {
      const unpackObservation = await runUnpack(
        fixture,
        join(fixture, `dist-unpack-${mode}`),
        undefined,
        false,
        mode
      );
      const webpackObservation = await runWebpack(
        fixture,
        join(fixture, `dist-webpack-${mode}`),
        undefined,
        mode
      );
      assert.deepEqual(unpackObservation, webpackObservation);
      assert.equal(unpackObservation.chunkModuleCount, expectedCount);
    }
  } finally {
    delete (globalThis as { CONCATENATE_MODULES_RESULT?: number }).CONCATENATE_MODULES_RESULT;
    await rm(fixture, { recursive: true, force: true });
  }
});

test("concatenated module output invalidates across cacheUnaffected rebuilds", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-concatenate-cache-"));
  const outputPath = join(fixture, "dist");
  let compiler: Compiler | undefined;
  try {
    await writeFixture(fixture, {
      "src/index.js": [
        "import { value } from './value.js';",
        "globalThis.CONCATENATE_MODULES_RESULT = value;",
        "export { value };"
      ].join("\n"),
      "src/value.js": "export const value = 41;"
    });
    compiler = unpack({
      context: fixture,
      mode: "none",
      entry: "./src/index.js",
      output: { path: outputPath },
      sourcemap: false,
      cache: { type: "memory", cacheUnaffected: true },
      experiments: { cacheUnaffected: true },
      optimization: { concatenateModules: true }
    });

    await runCompiler(compiler);
    executeEntry(join(outputPath, "main.js"));
    assert.equal(readRuntimeValue(), 41);

    await writeFile(join(fixture, "src/value.js"), "export const value = 42;");
    await runCompiler(compiler);
    executeEntry(join(outputPath, "main.js"));
    assert.equal(readRuntimeValue(), 42);
  } finally {
    if (compiler) await closeCompiler(compiler);
    delete (globalThis as { CONCATENATE_MODULES_RESULT?: number }).CONCATENATE_MODULES_RESULT;
    await rm(fixture, { recursive: true, force: true });
  }
});

// Ported from webpack 5.108.1: test/cases/scope-hoisting/import-order-11617.
test("concatenated modules evaluate static imports in webpack order", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-concatenate-order-"));
  try {
    await writeFixture(fixture, {
      "src/index.js": [
        "import './a.js';",
        "import { log } from './tracker.js';",
        "globalThis.CONCATENATE_MODULES_RESULT = log.join(',');",
        "export { log };"
      ].join("\n"),
      "src/a.js": [
        "import './b.js';",
        "import './c.js';",
        "import { track } from './tracker.js';",
        "track('a');",
        "export const a = true;"
      ].join("\n"),
      "src/b.js": [
        "import { track } from './tracker.js';",
        "track('b');",
        "export const b = true;"
      ].join("\n"),
      "src/c.js": [
        "import { track } from './tracker.js';",
        "track('c');",
        "export const c = true;"
      ].join("\n"),
      "src/tracker.js": [
        "export const log = [];",
        "export const track = (name) => log.push(name);"
      ].join("\n")
    });

    const unpackObservation = await runStringResultUnpack(
      fixture,
      join(fixture, "dist-unpack")
    );
    const webpackObservation = await runStringResultWebpack(
      fixture,
      join(fixture, "dist-webpack")
    );
    assert.equal(unpackObservation, webpackObservation);
    assert.equal(unpackObservation, "b,c,a");
  } finally {
    delete (globalThis as { CONCATENATE_MODULES_RESULT?: string }).CONCATENATE_MODULES_RESULT;
    await rm(fixture, { recursive: true, force: true });
  }
});

test("module concatenation bails out for modules referenced from different chunks", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-concatenate-bailout-"));
  try {
    await writeFixture(fixture, {
      "src/index.js": [
        "globalThis.CONCATENATE_MODULES_PROMISE = Promise.all([import('./a.js'), import('./b.js')]).then(([a, b]) => a.result + b.result);",
        "export default globalThis.CONCATENATE_MODULES_PROMISE;"
      ].join("\n"),
      "src/a.js": [
        "import { shared } from './shared.js';",
        "import { local } from './local.js';",
        "export const result = shared + local;"
      ].join("\n"),
      "src/b.js": [
        "import { shared } from './shared.js';",
        "export const result = shared + 2;"
      ].join("\n"),
      "src/shared.js": "export const shared = 40;",
      "src/local.js": "export const local = 2;"
    });

    const unpackObservation = await runAsyncChunksUnpack(fixture, join(fixture, "dist-unpack"));
    const webpackObservation = await runAsyncChunksWebpack(fixture, join(fixture, "dist-webpack"));
    assert.deepEqual(unpackObservation, webpackObservation);
    assert.deepEqual(unpackObservation, { chunkModuleCounts: [1, 2, 2], runtimeValue: 84 });
  } finally {
    delete (globalThis as { CONCATENATE_MODULES_PROMISE?: Promise<number> })
      .CONCATENATE_MODULES_PROMISE;
    await rm(fixture, { recursive: true, force: true });
  }
});

// Ported from webpack 5.108.1: test/cases/scope-hoisting/chained-reexport.
test("concatenated modules include static star reexport chains", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-concatenate-star-"));
  try {
    await writeFixture(fixture, {
      "src/index.js": [
        "import { named } from './c.js';",
        "globalThis.CONCATENATE_MODULES_RESULT = named;",
        "export { named };"
      ].join("\n"),
      "src/a.js": "export const named = 42;",
      "src/b.js": "export * from './a.js';",
      "src/c.js": "export { named } from './b.js';"
    });

    const unpackObservation = await runUnpack(fixture, join(fixture, "dist-unpack"), true);
    const webpackObservation = await runWebpack(fixture, join(fixture, "dist-webpack"), true);
    assert.deepEqual(unpackObservation, webpackObservation);
    assert.deepEqual(unpackObservation, { chunkModuleCount: 1, runtimeValue: 42 });
  } finally {
    delete (globalThis as { CONCATENATE_MODULES_RESULT?: number }).CONCATENATE_MODULES_RESULT;
    await rm(fixture, { recursive: true, force: true });
  }
});

async function runUnpack(
  context: string,
  outputPath: string,
  concatenateModules: boolean | undefined,
  sourcemap = false,
  mode: "production" | "development" | "none" = "none"
): Promise<{ chunkModuleCount: number; runtimeValue: number }> {
  let chunkModuleCount = -1;
  const stats = await new Promise<Stats>((resolve, reject) => {
    unpack({
      context,
      mode,
      entry: "./src/index.js",
      output: { path: outputPath },
      sourcemap,
      optimization: concatenateModules === undefined ? {} : { concatenateModules },
      plugins: [{
        apply(compiler) {
          let observedCompilation: Parameters<
            Parameters<typeof compiler.hooks.compilation.tap>[1]
          >[0];
          compiler.hooks.compilation.tap("ObserveConcatenatedModules", (compilation) => {
            observedCompilation = compilation;
          });
          compiler.hooks.done.tap("ObserveConcatenatedModules", () => {
            const [chunk] = observedCompilation.chunks;
            assert.ok(chunk);
            chunkModuleCount = [...observedCompilation.chunkGraph.getChunkModulesIterable(chunk)]
              .filter((module) => module.type.startsWith("javascript"))
              .length;
          });
        }
      }]
    }, (error, completedStats) => {
      if (error) reject(error);
      else if (completedStats) resolve(completedStats);
      else reject(new Error("Unpack completed without Stats"));
    });
  });
  assert.equal(stats.hasErrors(), false, JSON.stringify(stats.toJson().errors));
  executeEntry(join(outputPath, "main.js"));
  return {
    chunkModuleCount,
    runtimeValue: readRuntimeValue()
  };
}

async function runWebpack(
  context: string,
  outputPath: string,
  concatenateModules: boolean | undefined,
  mode: "production" | "development" | "none" = "none"
): Promise<{ chunkModuleCount: number; runtimeValue: number }> {
  let chunkModuleCount = -1;
  const compiler = webpack({
    context,
    mode,
    entry: "./src/index.js",
    output: { path: outputPath },
    devtool: false,
    optimization: concatenateModules === undefined
      ? { minimize: false }
      : { concatenateModules, minimize: false },
    plugins: [{
      apply(currentCompiler: WebpackCompiler) {
        currentCompiler.hooks.done.tap("ObserveConcatenatedModules", (stats) => {
          const [chunk] = stats.compilation.chunks;
          assert.ok(chunk);
          chunkModuleCount = [...stats.compilation.chunkGraph.getChunkModulesIterable(chunk)]
            .filter((module) => module.type.startsWith("javascript"))
            .length;
        });
      }
    }]
  });
  try {
    const stats = await new Promise<WebpackStats>((resolve, reject) => {
      compiler.run((error, completedStats) => {
        if (error) reject(error);
        else if (completedStats) resolve(completedStats);
        else reject(new Error("webpack completed without Stats"));
      });
    });
    assert.equal(stats.hasErrors(), false, stats.toString());
    executeEntry(join(outputPath, "main.js"));
    return {
      chunkModuleCount,
      runtimeValue: readRuntimeValue()
    };
  } finally {
    await new Promise<void>((resolve, reject) => {
      compiler.close((error) => error ? reject(error) : resolve());
    });
  }
}

function executeEntry(path: string): void {
  delete (globalThis as { CONCATENATE_MODULES_RESULT?: number }).CONCATENATE_MODULES_RESULT;
  const require = createRequire(import.meta.url);
  delete require.cache[require.resolve(path)];
  require(path);
}

function readRuntimeValue(): number {
  const value = (globalThis as { CONCATENATE_MODULES_RESULT?: number })
    .CONCATENATE_MODULES_RESULT;
  if (value === undefined) {
    throw new Error("bundle did not publish CONCATENATE_MODULES_RESULT");
  }
  return value;
}

async function runStringResultUnpack(context: string, outputPath: string): Promise<string> {
  const stats = await new Promise<Stats>((resolve, reject) => {
    unpack({
      context,
      mode: "none",
      entry: "./src/index.js",
      output: { path: outputPath },
      sourcemap: false,
      optimization: { concatenateModules: true }
    }, (error, completedStats) => {
      if (error) reject(error);
      else if (completedStats) resolve(completedStats);
      else reject(new Error("Unpack completed without Stats"));
    });
  });
  assert.equal(stats.hasErrors(), false, JSON.stringify(stats.toJson().errors));
  executeEntry(join(outputPath, "main.js"));
  return readStringRuntimeValue();
}

async function runStringResultWebpack(context: string, outputPath: string): Promise<string> {
  const compiler = webpack({
    context,
    mode: "none",
    entry: "./src/index.js",
    output: { path: outputPath },
    devtool: false,
    optimization: { concatenateModules: true, minimize: false }
  });
  try {
    const stats = await new Promise<WebpackStats>((resolve, reject) => {
      compiler.run((error, completedStats) => {
        if (error) reject(error);
        else if (completedStats) resolve(completedStats);
        else reject(new Error("webpack completed without Stats"));
      });
    });
    assert.equal(stats.hasErrors(), false, stats.toString());
    executeEntry(join(outputPath, "main.js"));
    return readStringRuntimeValue();
  } finally {
    await new Promise<void>((resolve, reject) => {
      compiler.close((error) => error ? reject(error) : resolve());
    });
  }
}

function readStringRuntimeValue(): string {
  const value = (globalThis as { CONCATENATE_MODULES_RESULT?: string })
    .CONCATENATE_MODULES_RESULT;
  if (value === undefined) {
    throw new Error("bundle did not publish CONCATENATE_MODULES_RESULT");
  }
  return value;
}

async function runAsyncChunksUnpack(
  context: string,
  outputPath: string
): Promise<{ chunkModuleCounts: number[]; runtimeValue: number }> {
  let chunkModuleCounts: number[] = [];
  const stats = await new Promise<Stats>((resolve, reject) => {
    unpack({
      context,
      mode: "none",
      entry: "./src/index.js",
      output: { path: outputPath },
      sourcemap: false,
      optimization: { concatenateModules: true },
      plugins: [{
        apply(compiler) {
          let observedCompilation: Parameters<
            Parameters<typeof compiler.hooks.compilation.tap>[1]
          >[0];
          compiler.hooks.compilation.tap("ObserveConcatenationBailout", (compilation) => {
            observedCompilation = compilation;
          });
          compiler.hooks.done.tap("ObserveConcatenationBailout", () => {
            chunkModuleCounts = [...observedCompilation.chunks]
              .map((chunk) => [...observedCompilation.chunkGraph.getChunkModulesIterable(chunk)]
                .filter((module) => module.type.startsWith("javascript"))
                .length)
              .sort((left, right) => left - right);
          });
        }
      }]
    }, (error, completedStats) => {
      if (error) reject(error);
      else if (completedStats) resolve(completedStats);
      else reject(new Error("Unpack completed without Stats"));
    });
  });
  assert.equal(stats.hasErrors(), false, JSON.stringify(stats.toJson().errors));
  executeEntry(join(outputPath, "main.js"));
  return { chunkModuleCounts, runtimeValue: await readAsyncRuntimeValue() };
}

async function runAsyncChunksWebpack(
  context: string,
  outputPath: string
): Promise<{ chunkModuleCounts: number[]; runtimeValue: number }> {
  let chunkModuleCounts: number[] = [];
  const compiler = webpack({
    context,
    mode: "none",
    target: "node",
    entry: "./src/index.js",
    output: { path: outputPath },
    devtool: false,
    optimization: { concatenateModules: true, minimize: false },
    plugins: [{
      apply(currentCompiler: WebpackCompiler) {
        currentCompiler.hooks.done.tap("ObserveConcatenationBailout", (stats) => {
          chunkModuleCounts = [...stats.compilation.chunks]
            .map((chunk) => [...stats.compilation.chunkGraph.getChunkModulesIterable(chunk)]
              .filter((module) => module.type.startsWith("javascript"))
              .length)
            .sort((left, right) => left - right);
        });
      }
    }]
  });
  try {
    const stats = await new Promise<WebpackStats>((resolve, reject) => {
      compiler.run((error, completedStats) => {
        if (error) reject(error);
        else if (completedStats) resolve(completedStats);
        else reject(new Error("webpack completed without Stats"));
      });
    });
    assert.equal(stats.hasErrors(), false, stats.toString());
    executeEntry(join(outputPath, "main.js"));
    return { chunkModuleCounts, runtimeValue: await readAsyncRuntimeValue() };
  } finally {
    await new Promise<void>((resolve, reject) => {
      compiler.close((error) => error ? reject(error) : resolve());
    });
  }
}

function readAsyncRuntimeValue(): Promise<number> {
  const value = (globalThis as { CONCATENATE_MODULES_PROMISE?: Promise<number> })
    .CONCATENATE_MODULES_PROMISE;
  if (value === undefined) {
    throw new Error("bundle did not publish CONCATENATE_MODULES_PROMISE");
  }
  return value;
}

async function writeFixture(root: string, files: Record<string, string>): Promise<void> {
  for (const [name, source] of Object.entries(files)) {
    const path = join(root, name);
    await mkdir(dirname(path), { recursive: true });
    await writeFile(path, source);
  }
}

function runCompiler(compiler: Compiler): Promise<Stats> {
  return new Promise((resolve, reject) => {
    compiler.run((error, stats) => {
      if (error) reject(error);
      else if (stats) resolve(stats);
      else reject(new Error("compiler completed without Stats"));
    });
  });
}

function closeCompiler(compiler: Compiler): Promise<void> {
  return new Promise((resolve, reject) => {
    compiler.close((error) => error ? reject(error) : resolve());
  });
}
