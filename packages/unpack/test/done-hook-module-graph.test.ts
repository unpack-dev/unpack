import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

import unpack from "@unpack-js/core";
import webpack from "webpack";
import type { Chunk as WebpackChunk } from "webpack";
import type {
  Chunk,
  ChunkGraph,
  Compiler,
  Module,
  ModuleGraphConnection,
  Stats
} from "@unpack-js/core";

// Ported from webpack's Compiler hook integration coverage: done is an
// AsyncSeriesHook and the final run callback observes it as completed.
test("done taps run serially before the run callback", async () => {
  const fixture = await createGraphFixture();
  const compiler = createCompiler(fixture);
  const events: string[] = [];
  const seenStats: Stats[] = [];

  compiler.hooks.done.tap({ name: "last", stage: 10 }, (stats) => {
    seenStats.push(stats);
    events.push("sync");
  });
  compiler.hooks.done.tapPromise(
    { name: "promise", before: "last" },
    async (stats) => {
      seenStats.push(stats);
      events.push("promise:start");
      await delay(5);
      events.push("promise:end");
    }
  );
  compiler.hooks.done.tapAsync(
    { name: "callback", before: "promise" },
    (stats, done) => {
      seenStats.push(stats);
      events.push("callback:start");
      setTimeout(() => {
        events.push("callback:end");
        done();
      }, 5);
    }
  );

  try {
    const stats = await runCompiler(compiler, () => events.push("run:callback"));
    assert.deepEqual(events, [
      "callback:start",
      "callback:end",
      "promise:start",
      "promise:end",
      "sync",
      "run:callback"
    ]);
    assert.equal(seenStats.length, 3);
    assert.equal(seenStats.every((seen) => seen === stats), true);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("finishModules taps run after make and before done", async () => {
  const fixture = await createGraphFixture();
  const compiler = createCompiler(fixture);
  const events: string[] = [];
  let finishModulesCompilation: import("@unpack-js/core").Compilation | undefined;
  let finishedModule: Module | undefined;
  let capturedModules: ReadonlySet<Module> | undefined;
  let capturedModuleGraph: import("@unpack-js/core").ModuleGraph | undefined;

  compiler.hooks.compilation.tap("observe compilation", (compilation) => {
    finishModulesCompilation = compilation;
    capturedModules = compilation.modules;
    capturedModuleGraph = compilation.moduleGraph;
    events.push("compilation");
    assert.equal(compilation.modules.size, 0);
    compilation.hooks.finishModules.tap("early", () => events.push("early"));
    compilation.hooks.finishModules.tap("target", () => events.push("target"));
    compilation.hooks.finishModules.tap(
      { name: "before target", before: "target" },
      () => events.push("before target")
    );
    compilation.hooks.finishModules.tapAsync("async modules", (modules, done) => {
      assert.equal(modules, capturedModules);
      assert.equal(compilation.moduleGraph, capturedModuleGraph);
      finishedModule = modules.values().next().value;
      events.push(`finishModules:${modules.size}:start`);
      setTimeout(() => {
        events.push("finishModules:end");
        done();
      }, 5);
    });
  });
  compiler.hooks.done.tap("done", (stats) => {
    events.push("done");
    assert.equal(stats.compilation, finishModulesCompilation);
    assert.equal(stats.compilation.modules, capturedModules);
    assert.equal(stats.compilation.moduleGraph, capturedModuleGraph);
    assert.equal(stats.compilation.modules.has(finishedModule!), true);
    assert.equal(stats.compilation.modules.size, finishModulesCompilation?.modules.size);
  });

  try {
    await runCompiler(compiler, () => events.push("run:callback"));
    assert.deepEqual(events, [
      "compilation",
      "early",
      "before target",
      "target",
      "finishModules:4:start",
      "finishModules:end",
      "done",
      "run:callback"
    ]);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("finishModules exposes a live webpack-shaped ModuleGraph", async () => {
  const fixture = await createGraphFixture();
  const unpackCompiler = createCompiler(fixture);
  const webpackCompiler = webpack({
    context: fixture,
    mode: "none",
    entry: "./src/index.js",
    output: { path: join(fixture, "dist-webpack") },
    devtool: false,
    optimization: {
      concatenateModules: false,
      providedExports: false,
      usedExports: false
    }
  });
  let unpackInspected = false;
  let webpackInspected = false;

  unpackCompiler.hooks.compilation.tap("observe compilation", (compilation) => {
    const moduleGraph = compilation.moduleGraph;
    assert.equal(compilation.modules.size, 0);
    compilation.hooks.finishModules.tap("inspect live module graph", (modules) => {
      assert.equal(compilation.moduleGraph, moduleGraph);
      assert.equal(modules.size, 4);
      const entry = findModule(modules, "/src/index.js");
      const left = findModule(modules, "/src/left.js");
      const leftConnection = [...moduleGraph.getOutgoingConnections(entry)]
        .find((connection) => connection.module === left);
      assert.ok(leftConnection);
      assert.equal(moduleGraph.getConnection(leftConnection.dependency), leftConnection);
      assert.equal(moduleGraph.getModule(leftConnection.dependency), left);
      unpackInspected = true;
    });
  });
  webpackCompiler.hooks.compilation.tap("observe compilation", (compilation) => {
    const moduleGraph = compilation.moduleGraph;
    assert.equal(compilation.modules.size, 0);
    compilation.hooks.finishModules.tap("inspect live module graph", (modules) => {
      assert.equal(compilation.moduleGraph, moduleGraph);
      const finishedModules = [...modules];
      assert.equal(finishedModules.length, 4);
      const entry = finishedModules.find((module) =>
        (module as { resource?: string }).resource?.endsWith(join("src", "index.js"))
      );
      const left = finishedModules.find((module) =>
        (module as { resource?: string }).resource?.endsWith(join("src", "left.js"))
      );
      assert.ok(entry);
      assert.ok(left);
      const leftConnection = [...moduleGraph.getOutgoingConnections(entry)]
        .find((connection) => connection.module === left);
      assert.ok(leftConnection);
      assert.ok(leftConnection.dependency);
      assert.equal(moduleGraph.getConnection(leftConnection.dependency), leftConnection);
      assert.equal(moduleGraph.getModule(leftConnection.dependency), left);
      webpackInspected = true;
    });
  });

  try {
    await runCompiler(unpackCompiler);
    await new Promise<void>((resolve, reject) => {
      webpackCompiler.run((error) => (error ? reject(error) : resolve()));
    });
    assert.equal(unpackInspected, true);
    assert.equal(webpackInspected, true);
  } finally {
    await closeCompiler(unpackCompiler);
    await new Promise<void>((resolve, reject) => {
      webpackCompiler.close((error) => (error ? reject(error) : resolve()));
    });
    await rm(fixture, { recursive: true, force: true });
  }
});

test("finishModules failures abort sealing and preserve the hook error", async () => {
  const fixture = await createGraphFixture();
  const compiler = createCompiler(fixture);
  const expected = new Error("finishModules failed");
  let doneCalled = false;

  compiler.hooks.compilation.tap("register failure", (compilation) => {
    compilation.hooks.finishModules.tapPromise("failure", async () => {
      throw expected;
    });
  });
  compiler.hooks.done.tap("must not run", () => { doneCalled = true; });

  try {
    const observation = await observeCompilerRun(compiler);
    assert.equal(observation.error, expected);
    assert.equal(observation.stats, undefined);
    assert.equal(doneCalled, false);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

// Ported from webpack's Compiler.run behavior: errors from done abort the
// successful callback path and do not provide Stats to the final callback.
test("done errors are delivered as run errors without Stats", async () => {
  const fixture = await createGraphFixture();
  const compiler = createCompiler(fixture);
  const expected = new Error("done failed");
  compiler.hooks.done.tap("failure", () => {
    throw expected;
  });

  try {
    const observation = await observeCompilerRun(compiler);
    assert.equal(observation.error, expected);
    assert.equal(observation.stats, undefined);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

// Ported from webpack ModuleGraph's public query contract. The assertions use
// a completed real compilation, as webpack plugins do, rather than a mock graph.
test("done exposes webpack-shaped ModuleGraph queries with cached grouping", async () => {
  const fixture = await createGraphFixture();
  const compiler = createCompiler(fixture);
  let inspected = false;

  compiler.hooks.done.tap("inspect module graph", (stats) => {
    const { moduleGraph, modules } = stats.compilation;
    assert.equal(modules.size, 4);

    const entry = findModule(modules, "/src/index.js");
    const left = findModule(modules, "/src/left.js");
    const right = findModule(modules, "/src/right.js");
    const shared = findModule(modules, "/src/shared.js");

    const entryOutgoing = moduleGraph.getOutgoingConnections(entry);
    assert.equal(entryOutgoing.size > 0, true);
    for (const connection of entryOutgoing) {
      assertConnection(connection);
      assert.equal(connection.originModule, entry);
      assert.equal(moduleGraph.getConnection(connection.dependency), connection);
      assert.equal(moduleGraph.getModule(connection.dependency), connection.module);
      assert.equal(moduleGraph.getResolvedModule(connection.dependency), connection.module);
      assert.equal(moduleGraph.getOrigin(connection.dependency), entry);
      assert.equal(moduleGraph.getResolvedOrigin(connection.dependency), entry);
      assert.equal(moduleGraph.getParentModule(connection.dependency), entry);
    }

    const outgoingByModule = moduleGraph.getOutgoingConnectionsByModule(entry);
    assert.ok(outgoingByModule);
    assert.equal(outgoingByModule, moduleGraph.getOutgoingConnectionsByModule(entry));
    assert.equal(outgoingByModule.has(left), true);
    assert.equal(outgoingByModule.has(right), true);

    const sharedIncoming = moduleGraph.getIncomingConnections(shared);
    const incomingOrigins = new Set(
      [...sharedIncoming].map((connection) => connection.originModule)
    );
    assert.equal(incomingOrigins.has(left), true);
    assert.equal(incomingOrigins.has(right), true);

    const incomingByOrigin = moduleGraph.getIncomingConnectionsByOriginModule(shared);
    assert.equal(
      incomingByOrigin,
      moduleGraph.getIncomingConnectionsByOriginModule(shared)
    );
    assert.equal(incomingByOrigin.has(left), true);
    assert.equal(incomingByOrigin.has(right), true);
    assert.equal(incomingOrigins.has(moduleGraph.getIssuer(shared) ?? null), true);

    assert.deepEqual(moduleGraph.getProvidedExports(shared), ["shared"]);
    assert.equal(moduleGraph.isExportProvided(shared, "shared"), true);
    assert.equal(moduleGraph.isExportProvided(shared, "missing"), false);
    assert.equal(
      moduleGraph.getExportsInfo(shared),
      moduleGraph.getExportsInfo(shared)
    );
    assert.equal(
      moduleGraph.getExportInfo(shared, "shared"),
      moduleGraph.getReadOnlyExportInfo(shared, "shared")
    );
    assert.equal(moduleGraph.getExportInfo(shared, "shared").provided, true);
    assert.deepEqual(moduleGraph.getUsedExports(shared), new Set(["shared"]));
    inspected = true;
  });

  try {
    const stats = await runCompiler(compiler);
    assert.equal(stats.hasErrors(), false);
    assert.equal(inspected, true);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("optimization providedExports and usedExports control ModuleGraph export metadata", async () => {
  const fixture = await createGraphFixture();
  const compiler = unpack({
    context: fixture,
    mode: "production",
    entry: "./src/index.js",
    output: { path: join(fixture, "dist") },
    sourcemap: false,
    optimization: { providedExports: false, usedExports: false }
  });
  assert.ok(compiler);
  let inspected = false;
  compiler.hooks.done.tap("inspect disabled export analysis", (stats) => {
    const shared = findModule(stats.compilation.modules, "/src/shared.js");
    assert.equal(stats.compilation.moduleGraph.getProvidedExports(shared), null);
    assert.equal(stats.compilation.moduleGraph.isExportProvided(shared, "shared"), null);
    assert.equal(stats.compilation.moduleGraph.getUsedExports(shared), null);
    inspected = true;
  });

  try {
    await runCompiler(compiler);
    assert.equal(inspected, true);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

// Ported from webpack ChunkGraph's public query contract. A dynamic import is
// used so both initial and async Chunk membership can be inspected.
test("done exposes webpack-shaped ChunkGraph membership queries", async () => {
  const fixture = await createChunkGraphFixture();
  const compiler = createCompiler(fixture);
  let inspected = false;
  let liveChunkGraph: ChunkGraph | undefined;

  compiler.hooks.done.tap("inspect chunk graph", (stats) => {
    const { chunkGraph, modules } = stats.compilation;
    assert.equal(stats.compilation.chunkGraph, chunkGraph);
    liveChunkGraph = chunkGraph;
    assert.equal(modules.size, 3);

    const entry = findModule(modules, "/src/index.js");
    const shared = findModule(modules, "/src/shared.js");
    const lazy = findModule(modules, "/src/lazy.js");
    const [initialChunk] = chunkGraph.getModuleChunks(entry);
    const [asyncChunk] = chunkGraph.getModuleChunks(lazy);
    assert.ok(initialChunk);
    assert.ok(asyncChunk);
    assert.notEqual(initialChunk, asyncChunk);

    assert.deepEqual(chunkGraph.getModuleChunks(entry), [initialChunk]);
    const entryChunksIterable = chunkGraph.getModuleChunksIterable(entry);
    assert.equal(
      chunkGraph.getOrderedModuleChunksIterable(entry, compareChunks),
      entryChunksIterable
    );
    assert.deepEqual([...chunkGraph.getModuleChunksIterable(lazy)], [asyncChunk]);
    assert.equal(chunkGraph.getNumberOfModuleChunks(shared), 1);
    assert.equal(chunkGraph.isModuleInChunk(shared, initialChunk), true);
    assert.equal(chunkGraph.isModuleInChunk(shared, asyncChunk), false);
    assert.equal(chunkGraph.isModuleInChunk(lazy, asyncChunk), true);

    const initialModules = chunkGraph.getChunkModules(initialChunk);
    const initialModulesIterable = chunkGraph.getChunkModulesIterable(initialChunk);
    assert.equal(
      chunkGraph.getOrderedChunkModulesIterable(initialChunk, compareModules),
      initialModulesIterable
    );
    assert.deepEqual(new Set(initialModules), new Set([entry, shared]));
    assert.equal(chunkGraph.getChunkModules(initialChunk), initialModules);
    assert.equal(chunkGraph.getNumberOfChunkModules(initialChunk), 2);
    assert.deepEqual([...chunkGraph.getChunkModulesIterable(asyncChunk)], [lazy]);

    assert.deepEqual(
      [...chunkGraph.getOrderedChunkModulesIterable(initialChunk, compareModules)],
      [...initialModules].sort(compareModules)
    );
    assert.equal(
      chunkGraph.getOrderedChunkModules(initialChunk, compareModules),
      chunkGraph.getOrderedChunkModules(initialChunk, compareModules)
    );
    assert.deepEqual(
      [...chunkGraph.getOrderedModuleChunksIterable(entry, compareChunks)],
      [initialChunk]
    );
    assert.equal(typeof chunkGraph.getModuleId(entry), "string");
    assert.equal(initialChunk.id, "main");
    inspected = true;
  });
  compiler.hooks.done.tapPromise("observe live chunk graph", async (stats) => {
    assert.equal(stats.compilation.chunkGraph, liveChunkGraph);
  });

  try {
    const stats = await runCompiler(compiler);
    assert.equal(stats.hasErrors(), false);
    assert.equal(inspected, true);
    assert.equal(stats.compilation.chunkGraph, liveChunkGraph);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("ChunkGraph ordered iterables preserve webpack live identity", async () => {
  const fixture = await createChunkGraphFixture();
  const unpackCompiler = unpack({
    context: fixture,
    mode: "production",
    entry: { main: "./src/index.js", other: "./src/other.js" },
    output: { path: join(fixture, "dist-unpack") },
    sourcemap: false
  });
  let unpackInspected = false;

  unpackCompiler.hooks.done.tap("inspect chunk graph identity", (stats) => {
    const { chunkGraph, modules } = stats.compilation;
    const shared = findModule(modules, "/src/shared.js");
    const cachedChunks = chunkGraph.getModuleChunks(shared);
    assert.equal(cachedChunks.length, 2);
    const chunksIterable = chunkGraph.getModuleChunksIterable(shared);
    const comparator = (left: Chunk, right: Chunk): number =>
      compareStrings(String(right.id), String(left.id));
    const expected = [...cachedChunks].sort(comparator);
    assert.equal(
      chunkGraph.getOrderedModuleChunksIterable(shared, comparator),
      chunksIterable
    );
    const reorderedChunks = chunkGraph.getModuleChunks(shared);
    assert.notEqual(reorderedChunks, cachedChunks);
    assert.deepEqual(reorderedChunks, expected);
    const lazy = findModule(modules, "/src/lazy.js");
    const cachedLazyChunks = chunkGraph.getModuleChunks(lazy);
    chunkGraph.getOrderedModuleChunksIterable(lazy, comparator);
    assert.equal(chunkGraph.getModuleChunks(lazy), cachedLazyChunks);
    unpackInspected = true;
  });

  try {
    await runCompiler(unpackCompiler);
    assert.equal(unpackInspected, true);
  } finally {
    await closeCompiler(unpackCompiler);
  }

  const compiler = webpack({
    context: fixture,
    mode: "production",
    entry: { main: "./src/index.js", other: "./src/other.js" },
    output: { path: join(fixture, "dist-webpack") },
    devtool: false,
    optimization: {
      concatenateModules: false,
      innerGraph: false,
      minimize: false,
      providedExports: false,
      usedExports: false
    }
  });
  let inspected = false;

  compiler.hooks.done.tap("inspect chunk graph identity", (stats) => {
    const { chunkGraph, chunks: compilationChunks } = stats.compilation;
    const modules = [...compilationChunks].flatMap((chunk) =>
      [...chunkGraph.getChunkModulesIterable(chunk)]
    );
    const shared = modules.find((candidate) =>
      (candidate as { resource?: string }).resource?.endsWith(join("src", "shared.js"))
    );
    assert.ok(shared, modules.map((module) => module.identifier()).join("\n"));
    const cachedChunks = chunkGraph.getModuleChunks(shared);
    assert.equal(cachedChunks.length, 2);
    const chunks = chunkGraph.getModuleChunksIterable(shared);
    const comparator = (left: WebpackChunk, right: WebpackChunk): -1 | 0 | 1 =>
      compareStrings(String(right.id), String(left.id));
    const expected = [...cachedChunks].sort(comparator);
    assert.equal(
      chunkGraph.getOrderedModuleChunksIterable(shared, comparator),
      chunks
    );
    const reorderedChunks = chunkGraph.getModuleChunks(shared);
    assert.notEqual(reorderedChunks, cachedChunks);
    assert.deepEqual(reorderedChunks, expected);
    const lazy = modules.find((candidate) =>
      (candidate as { resource?: string }).resource?.endsWith(join("src", "lazy.js"))
    );
    assert.ok(lazy);
    const cachedLazyChunks = chunkGraph.getModuleChunks(lazy);
    chunkGraph.getOrderedModuleChunksIterable(lazy, comparator);
    assert.equal(chunkGraph.getModuleChunks(lazy), cachedLazyChunks);
    const [chunk] = chunks;
    assert.ok(chunk);
    const chunkModules = chunkGraph.getChunkModulesIterable(chunk);
    assert.equal(
      chunkGraph.getOrderedChunkModulesIterable(chunk, (left, right) =>
        compareStrings(left.identifier(), right.identifier())
      ),
      chunkModules
    );
    inspected = true;
  });

  try {
    await new Promise<void>((resolve, reject) => {
      compiler.run((error) => (error ? reject(error) : resolve()));
    });
    assert.equal(inspected, true);
  } finally {
    await new Promise<void>((resolve, reject) => {
      compiler.close((error) => (error ? reject(error) : resolve()));
    });
    await rm(fixture, { recursive: true, force: true });
  }
});

function assertConnection(connection: ModuleGraphConnection): void {
  assert.equal(connection.resolvedOriginModule, connection.originModule);
  assert.equal(connection.resolvedModule, connection.module);
  assert.equal(connection.active, true);
  assert.equal(connection.getActiveState(), true);
  assert.equal(connection.isActive(), true);
  assert.equal(connection.isTargetActive(), true);
}

function findModule(modules: ReadonlySet<Module>, suffix: string): Module {
  const normalizedSuffix = suffix.replaceAll("/", join("/"));
  const module = [...modules].find((candidate) =>
    candidate.resource.endsWith(normalizedSuffix)
  );
  assert.ok(module, `expected module ending in ${suffix}`);
  return module;
}

function compareModules(left: Module, right: Module): number {
  return left.identifier().localeCompare(right.identifier());
}

function compareChunks(left: Chunk, right: Chunk): number {
  return String(left.id).localeCompare(String(right.id));
}

function compareStrings(left: string, right: string): -1 | 0 | 1 {
  return left < right ? -1 : left > right ? 1 : 0;
}

function createCompiler(context: string): Compiler {
  return unpack({
    context,
    entry: "./src/index.js",
    output: { path: join(context, "dist") },
    sourcemap: false
  });
}

async function createGraphFixture(): Promise<string> {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-module-graph-"));
  await writeFixtureFile(
    join(fixture, "src/index.js"),
    'export { left } from "./left.js";\nexport { right } from "./right.js";\n'
  );
  await writeFixtureFile(
    join(fixture, "src/left.js"),
    'import { shared } from "./shared.js";\nexport const left = shared;\n'
  );
  await writeFixtureFile(
    join(fixture, "src/right.js"),
    'import { shared } from "./shared.js";\nexport const right = shared;\n'
  );
  await writeFixtureFile(
    join(fixture, "src/shared.js"),
    'export const shared = "shared";\n'
  );
  return fixture;
}

async function createChunkGraphFixture(): Promise<string> {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-chunk-graph-"));
  await writeFixtureFile(
    join(fixture, "src/index.js"),
    'import { shared } from "./shared.js";\nconsole.log(shared);\nexport const value = shared;\nexport const load = () => import("./lazy.js");\n'
  );
  await writeFixtureFile(
    join(fixture, "src/shared.js"),
    'export const shared = "shared";\n'
  );
  await writeFixtureFile(
    join(fixture, "src/lazy.js"),
    'export const lazy = "lazy";\n'
  );
  await writeFixtureFile(
    join(fixture, "src/other.js"),
    'import { shared } from "./shared.js";\nconsole.log(shared);\nexport const other = shared;\n'
  );
  return fixture;
}

async function writeFixtureFile(path: string, contents: string): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, contents);
}

function runCompiler(compiler: Compiler, onCallback?: () => void): Promise<Stats> {
  return new Promise((resolve, reject) => {
    compiler.run((error, stats) => {
      onCallback?.();
      if (error) reject(error);
      else if (!stats) reject(new Error("compiler completed without Stats"));
      else resolve(stats);
    });
  });
}

function observeCompilerRun(
  compiler: Compiler
): Promise<{ error: Error | null; stats: Stats | undefined }> {
  return new Promise((resolve) => {
    compiler.run((error, stats) => resolve({ error, stats }));
  });
}

function closeCompiler(compiler: Compiler): Promise<void> {
  return new Promise((resolve, reject) => {
    compiler.close((error) => (error ? reject(error) : resolve()));
  });
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}
