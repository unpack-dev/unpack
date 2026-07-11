import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

import unpack from "@unpack-js/core";
import type {
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
    assert.equal(moduleGraph.getUsedExports(shared), null);
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
