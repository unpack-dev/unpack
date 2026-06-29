import { mkdir, mkdtemp, readFile, realpath, rm, utimes, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

import unpack from "@unpack-js/core";
import type { Stats } from "@unpack-js/core";

test("emits assets through the ESM default API", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 42;"
  });
  const outputPath = join(fixture, "build");

  try {
    const { err, stats } = await runCompiler({
      context: fixture,
      entry: "./src/index.js",
      output: { path: outputPath }
    });

    assert.equal(err, null);
    assert.ok(stats);
    assert.equal(stats.hasErrors(), false);
    assert.deepEqual(
      stats.toJson().assets.map((asset) => asset.name).sort(),
      ["main.js", "main.js.map"]
    );
    assert.equal(stats.toJson().outputPath, outputPath);
    assert.match(await readFile(join(outputPath, "main.js"), "utf8"), /__webpack_require__/);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("supports object entries", async () => {
  const fixture = await createFixture({
    "src/a.js": "export const a = 'a';",
    "src/b.js": "export const b = 'b';"
  });

  try {
    const { err, stats } = await runCompiler({
      context: fixture,
      entry: {
        a: "./src/a.js",
        b: "./src/b.js"
      }
    });

    assert.equal(err, null);
    assert.ok(stats);
    assert.deepEqual(
      stats.toJson().assets.map((asset) => asset.name).sort(),
      ["a.js", "a.js.map", "b.js", "b.js.map"]
    );
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("unpack options callback closes the returned compiler", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });

  try {
    let compiler!: ReturnType<typeof unpack>;
    const callbackResult = new Promise<{ err: Error | null; stats?: Stats }>(
      (resolve) => {
        compiler = unpack(
          {
            context: fixture,
            entry: "./src/index.js"
          },
          (err, stats) => resolve({ err, stats })
        );
      }
    );

    const { err, stats } = await callbackResult;
    assert.equal(err, null);
    assert.equal(stats?.hasErrors(), false);
    const rerun = await runExistingCompiler(compiler);
    assert.equal(rerun.err?.name, "CompilerClosedError");
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("manual compiler remains reusable until close", async () => {
  const fixture = await createFixture({
    "src/index.js": "import './dep'; export const value = 1;",
    "src/dep.js": "globalThis.__unpackDep = true;"
  });

  try {
    const compiler = unpack(
      {
        context: fixture,
        entry: "./src/index.js"
      }
    );

    const first = await runExistingCompiler(compiler);
    const second = await runExistingCompiler(compiler);
    assert.equal(first.err, null);
    assert.equal(second.err, null);
    assert.deepEqual(second.stats?.toJson(), first.stats?.toJson());
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /__unpackDep/);
    await closeCompiler(compiler);
    assert.equal((await runExistingCompiler(compiler)).err?.name, "CompilerClosedError");
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("manual compiler rerun emits source edits", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 'before';"
  });
  const compiler = unpack({ context: fixture, entry: "./src/index.js" });
  const entry = join(fixture, "src/index.js");

  try {
    const first = await runExistingCompiler(compiler);
    assert.equal(first.err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /before/);

    await writeFile(entry, "export const value = 'after';", { encoding: "utf8" });
    const changedTime = new Date(Date.now() + 2000);
    await utimes(entry, changedTime, changedTime);

    const second = await runExistingCompiler(compiler);
    assert.equal(second.err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /after/);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("cache false disables module build cache reuse", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 'before';"
  });
  const entry = join(fixture, "src/index.js");
  const stableTime = new Date("2020-01-01T00:00:00.000Z");
  await utimes(entry, stableTime, stableTime);
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: false
  });

  try {
    const first = await runExistingCompiler(compiler);
    assert.equal(first.err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /before/);

    await writeFile(entry, "export const value = 'after';", { encoding: "utf8" });
    await utimes(entry, stableTime, stableTime);

    const second = await runExistingCompiler(compiler);
    assert.equal(second.err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /after/);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("snapshot module hash detects same-timestamp source edits", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 'before';"
  });
  const entry = join(fixture, "src/index.js");
  const stableTime = new Date("2020-01-01T00:00:00.000Z");
  await utimes(entry, stableTime, stableTime);
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: true,
    snapshot: {
      module: { timestamp: false, hash: true },
      buildDependencies: { timestamp: true, hash: true }
    }
  });

  try {
    const first = await runExistingCompiler(compiler);
    assert.equal(first.err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /before/);

    await writeFile(entry, "export const value = 'after';", { encoding: "utf8" });
    await utimes(entry, stableTime, stableTime);

    const second = await runExistingCompiler(compiler);
    assert.equal(second.err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /after/);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("omitted and production mode default module snapshots to timestamp plus hash", async () => {
  await assertSameTimestampModuleEditEmits("after", {});
  await assertSameTimestampModuleEditEmits("after", { mode: "production" });
});

test("development and none default module snapshots to timestamp only", async () => {
  await assertSameTimestampModuleEditEmits("before", { mode: "development" });
  await assertSameTimestampModuleEditEmits("before", { mode: "none" });
});

test("mode does not weaken build dependency snapshot defaults", async () => {
  await assertSameTimestampBuildDependencyEditEmits({
    snapshot: {
      resolveBuildDependencies: { timestamp: true, hash: false }
    }
  });
});

test("mode does not weaken resolve build dependency snapshot defaults", async () => {
  await assertSameTimestampBuildDependencyEditEmits({
    snapshot: {
      buildDependencies: { timestamp: true, hash: false }
    }
  });
});

test("default managed node_modules snapshots invalidate on package version changes", async () => {
  const fixture = await createNodeModulesFixture();
  const moduleFile = join(fixture, "node_modules/pkg/index.js");
  const packageJson = join(fixture, "node_modules/pkg/package.json");
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: true,
    snapshot: {
      module: { timestamp: false, hash: true }
    }
  });

  try {
    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /before/);

    await writeFile(moduleFile, "export const value = 'after';", { encoding: "utf8" });
    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /before/);

    await writeFile(packageJson, JSON.stringify({ name: "pkg", version: "2.0.0" }), {
      encoding: "utf8"
    });
    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /after/);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("unversioned managed packages fall back to module file snapshots", async () => {
  const fixture = await createFixture({
    "src/index.js": "import { value } from 'pkg/index.js'; export const result = value;",
    "node_modules/pkg/package.json": JSON.stringify({ name: "pkg" }),
    "node_modules/pkg/index.js": "export const value = 'before';"
  });
  const moduleFile = join(fixture, "node_modules/pkg/index.js");
  const stableTime = new Date("2020-01-01T00:00:00.000Z");
  await utimes(moduleFile, stableTime, stableTime);
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: true,
    snapshot: {
      module: { timestamp: false, hash: true }
    }
  });

  try {
    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /before/);

    await writeFile(moduleFile, "export const value = 'after';", { encoding: "utf8" });
    await utimes(moduleFile, stableTime, stableTime);
    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /after/);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("unmanaged path patterns override managed node_modules snapshots", async () => {
  const fixture = await createNodeModulesFixture();
  const moduleFile = join(fixture, "node_modules/pkg/index.js");
  const stableTime = new Date("2020-01-01T00:00:00.000Z");
  await utimes(moduleFile, stableTime, stableTime);
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: true,
    snapshot: {
      module: { timestamp: false, hash: true },
      unmanagedPaths: [join(fixture, "node_modules/pkg")]
    }
  });

  try {
    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /before/);

    await writeFile(moduleFile, "export const value = 'after';", { encoding: "utf8" });
    await utimes(moduleFile, stableTime, stableTime);
    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /after/);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("immutable path patterns bypass module snapshot file validation", async () => {
  const fixture = await createNodeModulesFixture();
  const moduleFile = join(fixture, "node_modules/pkg/index.js");
  const stableTime = new Date("2020-01-01T00:00:00.000Z");
  await utimes(moduleFile, stableTime, stableTime);
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: true,
    snapshot: {
      module: { timestamp: false, hash: true },
      immutablePaths: [/NODE_MODULES.PKG/i]
    }
  });

  try {
    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /before/);

    await writeFile(moduleFile, "export const value = 'after';", { encoding: "utf8" });
    await utimes(moduleFile, stableTime, stableTime);
    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /before/);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("accepts filesystem cache option shape", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;",
    "config/build.js": "export default {};"
  });
  const cacheOptions = {
    type: "filesystem" as const,
    cacheDirectory: ".cache/unpack",
    name: "test-cache",
    version: "v1",
    buildDependencies: {
      config: ["./config/build.js"]
    },
    maxMemoryGenerations: 2,
    idleTimeout: 10
  };
  const snapshot = {
    module: { timestamp: true, hash: false },
    resolve: { timestamp: true, hash: false },
    buildDependencies: { timestamp: true, hash: true }
  };

  try {
    const firstCompiler = unpack({
      context: fixture,
      entry: "./src/index.js",
      cache: cacheOptions,
      snapshot
    });
    const first = await runExistingCompiler(firstCompiler);
    await closeCompiler(firstCompiler);

    assert.equal(first.err, null);
    assert.equal(first.stats?.hasErrors(), false);
    assert.match(
      await readFile(join(fixture, ".cache/unpack/test-cache/container.json"), "utf8"),
      /UNPACK_PERSISTENT_CACHE/
    );
    assert.ok(await readFile(join(fixture, ".cache/unpack/test-cache/packs/modules.cbor")));

    const secondCompiler = unpack({
      context: fixture,
      entry: "./src/index.js",
      cache: cacheOptions,
      snapshot
    });
    const second = await runExistingCompiler(secondCompiler);
    await closeCompiler(secondCompiler);
    assert.equal(second.err, null);
    assert.deepEqual(second.stats?.toJson(), first.stats?.toJson());
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("filesystem cache flushes after idle timeout", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const cacheLocation = join(fixture, ".cache/unpack/idle");
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: {
      type: "filesystem",
      cacheLocation,
      idleTimeout: 30
    }
  });

  try {
    const result = await runExistingCompiler(compiler);
    assert.equal(result.err, null);
    await assert.rejects(readFile(join(cacheLocation, "container.json"), "utf8"));

    await delay(100);
    assert.match(
      await readFile(join(cacheLocation, "container.json"), "utf8"),
      /UNPACK_PERSISTENT_CACHE/
    );
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("compiler close waits for pending filesystem cache flush", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const cacheLocation = join(fixture, ".cache/unpack/close");
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: {
      type: "filesystem",
      cacheLocation,
      idleTimeout: 60_000
    }
  });

  try {
    const result = await runExistingCompiler(compiler);
    assert.equal(result.err, null);

    await closeCompiler(compiler);
    assert.match(
      await readFile(join(cacheLocation, "container.json"), "utf8"),
      /UNPACK_PERSISTENT_CACHE/
    );
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("watch performs initial build and close keeps compiler reusable", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const compiler = unpack({ context: fixture, entry: "./src/index.js" });

  try {
    const results = collectWatchResults();
    const first = results.next();
    const watching = compiler.watch({}, results.handler);
    const result = await first;
    assert.equal(result.err, null);
    assert.equal(result.stats?.hasErrors(), false);

    await closeWatching(watching);
    assert.equal((await runExistingCompiler(compiler)).err, null);
    await closeCompiler(compiler);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("watch invalidate triggers rebuild", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 'before';"
  });
  const compiler = unpack({ context: fixture, entry: "./src/index.js" });
  const entry = join(fixture, "src/index.js");

  try {
    const results = collectWatchResults();
    const first = results.next();
    const watching = compiler.watch({}, results.handler);
    assert.equal((await first).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /before/);

    const second = results.next();
    await writeFile(entry, "export const value = 'after';", { encoding: "utf8" });
    const changedTime = new Date(Date.now() + 2000);
    await utimes(entry, changedTime, changedTime);
    watching.invalidate();

    assert.equal((await second).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /after/);
    await closeWatching(watching);
    await closeCompiler(compiler);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("stats exposes watch dependency sets", async () => {
  const fixture = await createFixture({
    "src/index.js": "import './dep'; import './missing'; export const value = dep;",
    "src/dep.js": "export const dep = 'dep';"
  });
  const compiler = unpack({ context: fixture, entry: "./src/index.js" });

  try {
    const result = await runExistingCompiler(compiler);
    assert.equal(result.err, null);
    const json = result.stats?.toJson();
    const sourceRoot = await realpath(join(fixture, "src"));
    assert.ok(json);
    assert.equal(json.errors.length, 1);
    assert.deepEqual(json.watchDependencies.files.sort(), [
      join(sourceRoot, "dep.js"),
      join(sourceRoot, "index.js")
    ]);
    assert.deepEqual(json.watchDependencies.contexts, []);
    assert.ok(json.watchDependencies.missing.includes(join(sourceRoot, "missing")));
    assert.ok(json.watchDependencies.missing.includes(join(sourceRoot, "dep.ts")));
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("watch rebuilds when a watched dependency changes", async () => {
  const fixture = await createFixture({
    "src/index.js": "import { value } from './dep'; export const result = value;",
    "src/dep.js": "export const value = 'before';"
  });
  const compiler = unpack({ context: fixture, entry: "./src/index.js" });
  const dependency = join(fixture, "src/dep.js");

  try {
    const results = collectWatchResults();
    const first = results.next();
    const watching = compiler.watch({}, results.handler);
    assert.equal((await first).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /before/);

    const second = results.next();
    await writeFile(dependency, "export const value = 'after';", { encoding: "utf8" });
    const changedTime = new Date(Date.now() + 2000);
    await utimes(dependency, changedTime, changedTime);

    assert.equal((await second).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /after/);
    await closeWatching(watching);
    await closeCompiler(compiler);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("watch aggregateTimeout coalesces rapid changes", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 'initial';"
  });
  const compiler = unpack({ context: fixture, entry: "./src/index.js" });
  const entry = join(fixture, "src/index.js");

  try {
    const results = collectWatchResults();
    const first = results.next();
    const watching = compiler.watch({ aggregateTimeout: 50 }, results.handler);
    assert.equal((await first).err, null);

    const second = results.next();
    await writeFile(entry, "export const value = 'first';", { encoding: "utf8" });
    const firstTime = new Date(Date.now() + 2000);
    await utimes(entry, firstTime, firstTime);
    await writeFile(entry, "export const value = 'second';", { encoding: "utf8" });
    const secondTime = new Date(Date.now() + 4000);
    await utimes(entry, secondTime, secondTime);

    assert.equal((await second).err, null);
    await delay(150);
    assert.equal(results.calls(), 2);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /second/);
    await closeWatching(watching);
    await closeCompiler(compiler);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("watch ignored string prevents rebuilds from ignored files", async () => {
  const fixture = await createFixture({
    "src/index.js": "import { value } from './ignored'; export const result = value;",
    "src/ignored.js": "export const value = 'before';"
  });
  const compiler = unpack({ context: fixture, entry: "./src/index.js" });
  const ignored = join(fixture, "src/ignored.js");

  try {
    const results = collectWatchResults();
    const first = results.next();
    const watching = compiler.watch({ ignored: "ignored.js" }, results.handler);
    assert.equal((await first).err, null);

    await writeFile(ignored, "export const value = 'after';", { encoding: "utf8" });
    const changedTime = new Date(Date.now() + 2000);
    await utimes(ignored, changedTime, changedTime);
    await delay(150);

    assert.equal(results.calls(), 1);
    await closeWatching(watching);
    await closeCompiler(compiler);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("watch poll option rebuilds through polling", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 'before';"
  });
  const compiler = unpack({ context: fixture, entry: "./src/index.js" });
  const entry = join(fixture, "src/index.js");

  try {
    const results = collectWatchResults();
    const first = results.next();
    const watching = compiler.watch({ aggregateTimeout: 0, poll: 20 }, results.handler);
    assert.equal((await first).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /before/);

    const second = results.next();
    await writeFile(entry, "export const value = 'after';", { encoding: "utf8" });
    const changedTime = new Date(Date.now() + 2000);
    await utimes(entry, changedTime, changedTime);

    assert.equal((await second).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /after/);
    await closeWatching(watching);
    await closeCompiler(compiler);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("watch conflicts with run watch and compiler close", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const compiler = unpack({ context: fixture, entry: "./src/index.js" });

  try {
    const results = collectWatchResults();
    const first = results.next();
    const watching = compiler.watch({}, results.handler);
    assert.equal((await first).err, null);

    assert.equal((await runExistingCompiler(compiler)).err?.name, "ConcurrentRunError");

    const failedWatch = collectWatchResults();
    const failed = failedWatch.next();
    compiler.watch({}, failedWatch.handler);
    assert.equal((await failed).err?.name, "ConcurrentRunError");

    const closeResult = await closeCompilerResult(compiler);
    assert.equal(closeResult?.name, "CompilerRunningError");

    await closeWatching(watching);
    await closeCompiler(compiler);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("run callback is asynchronous", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const compiler = unpack({ context: fixture, entry: "./src/index.js" });
  let sync = true;

  try {
    const result = await new Promise<boolean>((resolve) => {
      compiler.run(() => {
        resolve(sync);
      });
      sync = false;
    });

    assert.equal(result, false);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("rejects concurrent runs on the same compiler", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const compiler = unpack({ context: fixture, entry: "./src/index.js" });

  try {
    const first = runExistingCompiler(compiler);
    const second = await runExistingCompiler(compiler);
    assert.equal(second.err?.name, "ConcurrentRunError");
    assert.equal((await first).err, null);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("compilation errors are reported in stats and still emit assets", async () => {
  const fixture = await createFixture({
    "src/index.js": "import {"
  });

  try {
    const { err, stats } = await runCompiler({
      context: fixture,
      entry: "./src/index.js"
    });

    assert.equal(err, null);
    assert.ok(stats);
    assert.equal(stats.hasErrors(), true);
    assert.match(stats.toJson().errors[0]?.message, /failed to parse/);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /throw new Error/);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("infrastructure logging is quiet by default", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const captured = captureConsole();

  try {
    const result = await runCompiler({
      context: fixture,
      entry: "./src/index.js"
    });

    assert.equal(result.err, null);
    assert.deepEqual(captured.all(), []);
  } finally {
    captured.restore();
    await rm(fixture, { recursive: true, force: true });
  }
});

test("infrastructure logging level info reports compiler runs outside stats", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const captured = captureConsole();

  try {
    const result = await runCompiler({
      context: fixture,
      entry: "./src/index.js",
      infrastructureLogging: {
        level: "info"
      }
    });

    assert.equal(result.err, null);
    assert.ok(result.stats);
    assert.equal("logs" in result.stats.toJson(), false);
    assert.deepEqual(captured.calls.info, [
      "[unpack.Compiler] run started",
      "[unpack.Compiler] run completed"
    ]);
    assert.deepEqual(captured.calls.log, []);
  } finally {
    captured.restore();
    await rm(fixture, { recursive: true, force: true });
  }
});

test("infrastructure logging level verbose reports compilation phases", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const captured = captureConsole();

  try {
    const result = await runCompiler({
      context: fixture,
      entry: "./src/index.js",
      infrastructureLogging: {
        level: "verbose"
      }
    });

    assert.equal(result.err, null);
    assert.deepEqual(captured.calls.log, [
      "[unpack.Compilation] make started",
      "[unpack.Compilation] make completed",
      "[unpack.Compilation] chunk graph build started",
      "[unpack.Compilation] chunk graph build completed",
      "[unpack.Compilation] asset creation started",
      "[unpack.Compilation] asset creation completed"
    ]);
  } finally {
    captured.restore();
    await rm(fixture, { recursive: true, force: true });
  }
});

test("top-level option validation throws synchronously", () => {
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        // @ts-expect-error intentionally testing runtime validation
        mode: "staging"
      }),
    /options.mode must be 'development', 'production', or 'none'/
  );
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        // @ts-expect-error intentionally testing runtime validation
        plugins: []
      }),
    /unknown option 'plugins'/
  );
  assert.throws(
    () =>
      unpack(
        // @ts-expect-error intentionally testing runtime validation
        null
      ),
    /options must be an object/
  );
});

test("infrastructure logging option validation throws synchronously", () => {
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        infrastructureLogging: {
          // @ts-expect-error intentionally testing runtime validation
          debug: true
        }
      }),
    /options.infrastructureLogging contains unknown option 'debug'/
  );
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        infrastructureLogging: {
          // @ts-expect-error intentionally testing runtime validation
          level: "debug"
        }
      }),
    /options.infrastructureLogging.level must be/
  );
});

test("cache and snapshot option validation throws synchronously", () => {
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        // @ts-expect-error intentionally testing runtime validation
        cache: "memory"
      }),
    /options.cache must be a boolean or an object/
  );
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        cache: {
          type: "memory",
          // @ts-expect-error intentionally testing runtime validation
          unknown: true
        }
      }),
    /options.cache contains unknown option 'unknown'/
  );
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        snapshot: {
          // @ts-expect-error intentionally testing runtime validation
          unknown: {}
        }
      }),
    /options.snapshot contains unknown option 'unknown'/
  );
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        snapshot: {
          resolve: {
            // @ts-expect-error intentionally testing runtime validation
            hash: "yes"
          }
        }
      }),
    /options.snapshot.resolve.hash must be a boolean/
  );
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        snapshot: {
          module: {
            // @ts-expect-error intentionally testing runtime validation
            timestamp: "yes"
          }
        }
      }),
    /options.snapshot.module.timestamp must be a boolean/
  );
  assert.doesNotThrow(() =>
    unpack({
      entry: "./src/index.js",
      snapshot: {
        resolve: {
          timestamp: false
        }
      }
    })
  );
  assert.doesNotThrow(() =>
    unpack({
      entry: "./src/index.js",
      mode: "production",
      snapshot: {
        resolve: {
          timestamp: false
        }
      }
    })
  );
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        snapshot: {
          module: {
            timestamp: false,
            hash: false
          }
        }
      }),
    /options.snapshot.module must enable timestamp or hash validation/
  );
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        mode: "development",
        snapshot: {
          resolve: {
            timestamp: false
          },
          resolveBuildDependencies: {
            timestamp: true,
            hash: false
          }
        }
      }),
    /options.snapshot.resolve must enable timestamp or hash validation/
  );
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        snapshot: {
          resolveBuildDependencies: {
            timestamp: false,
            hash: false
          }
        }
      }),
    /options.snapshot.resolveBuildDependencies must enable timestamp or hash validation/
  );
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        snapshot: {
          managedPaths: ["node_modules"]
        }
      }),
    /options.snapshot.managedPaths\[0\] must be an absolute path/
  );
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        snapshot: {
          immutablePaths: [/node_modules/g]
        }
      }),
    /options.snapshot.immutablePaths\[0\] RegExp flags must be empty or 'i'/
  );
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        snapshot: {
          immutablePaths: [/(?<=node_modules)pkg/]
        }
      }),
    /snapshot path RegExp .* is not supported by Rust regex/
  );
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        snapshot: {
          // @ts-expect-error intentionally testing runtime validation
          unmanagedPaths: "/absolute/path"
        }
      }),
    /options.snapshot.unmanagedPaths must be an array/
  );
  assert.doesNotThrow(() =>
    unpack({
      entry: "./src/index.js",
      snapshot: {
        managedPaths: ["/absolute/path"],
        immutablePaths: [/node_modules/i],
        unmanagedPaths: []
      }
    })
  );
});

test("watch option validation throws synchronously", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const compiler = unpack({ context: fixture, entry: "./src/index.js" });

  try {
    assert.throws(
      () =>
        compiler.watch(
          {
            // @ts-expect-error intentionally testing runtime validation
            followSymlinks: false
          },
          () => {}
        ),
      /watchOptions contains unknown option 'followSymlinks'/
    );
    assert.throws(
      () =>
        compiler.watch(
          {
            // @ts-expect-error intentionally testing runtime validation
            ignored: 1
          },
          () => {}
        ),
      /watchOptions.ignored must be a string or RegExp/
    );
    assert.throws(
      () =>
        compiler.watch(
          {
            // @ts-expect-error intentionally testing runtime validation
            ignored: ["ok", 1]
          },
          () => {}
        ),
      /watchOptions.ignored\[1\] must be a string or RegExp/
    );
    assert.throws(
      () =>
        compiler.watch(
          {
            // @ts-expect-error intentionally testing runtime validation
            poll: false
          },
          () => {}
        ),
      /watchOptions.poll must be true or a positive integer/
    );
    assert.throws(
      () =>
        compiler.watch(
          {
            poll: 0
          },
          () => {}
        ),
      /watchOptions.poll must be a positive integer/
    );

    const results = collectWatchResults();
    const first = results.next();
    const watching = compiler.watch(
      { ignored: [/unused/, "not-used"], poll: true },
      results.handler
    );
    assert.equal((await first).err, null);
    await closeWatching(watching);
    await closeCompiler(compiler);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

async function runCompiler(options: Parameters<typeof unpack>[0]) {
  return runExistingCompiler(unpack(options));
}

async function assertSameTimestampModuleEditEmits(
  expected: string,
  options: Partial<Parameters<typeof unpack>[0]>
) {
  const fixture = await createFixture({
    "src/index.js": "export const value = 'before';"
  });
  const entry = join(fixture, "src/index.js");
  const stableTime = new Date("2020-01-01T00:00:00.000Z");
  await utimes(entry, stableTime, stableTime);
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: true,
    ...options
  });

  try {
    const first = await runExistingCompiler(compiler);
    assert.equal(first.err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /before/);

    await writeFile(entry, "export const value = 'after';", { encoding: "utf8" });
    await utimes(entry, stableTime, stableTime);

    const second = await runExistingCompiler(compiler);
    assert.equal(second.err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), new RegExp(expected));
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
}

async function assertSameTimestampBuildDependencyEditEmits(
  options: Partial<Parameters<typeof unpack>[0]>
) {
  const fixture = await createFixture({
    "src/index.js": "export const value = 'before';",
    "config/build.js": "export default 'before';"
  });
  const entry = join(fixture, "src/index.js");
  const config = join(fixture, "config/build.js");
  const cacheLocation = join(fixture, ".cache/unpack/default");
  const stableTime = new Date("2020-01-01T00:00:00.000Z");
  await utimes(entry, stableTime, stableTime);
  await utimes(config, stableTime, stableTime);

  try {
    const firstCompiler = unpack({
      context: fixture,
      mode: "development",
      entry: "./src/index.js",
      cache: {
        type: "filesystem",
        cacheLocation,
        buildDependencies: {
          config: ["./config/build.js"]
        }
      },
      ...options
    });
    const first = await runExistingCompiler(firstCompiler);
    await closeCompiler(firstCompiler);
    assert.equal(first.err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /before/);

    await writeFile(entry, "export const value = 'after';", { encoding: "utf8" });
    await writeFile(config, "export default 'after';", { encoding: "utf8" });
    await utimes(entry, stableTime, stableTime);
    await utimes(config, stableTime, stableTime);

    const secondCompiler = unpack({
      context: fixture,
      mode: "development",
      entry: "./src/index.js",
      cache: {
        type: "filesystem",
        cacheLocation,
        buildDependencies: {
          config: ["./config/build.js"]
        }
      },
      ...options
    });
    const second = await runExistingCompiler(secondCompiler);
    await closeCompiler(secondCompiler);
    assert.equal(second.err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /after/);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
}

async function createNodeModulesFixture() {
  return createFixture({
    "src/index.js": "import { value } from 'pkg/index.js'; export const result = value;",
    "node_modules/pkg/package.json": JSON.stringify({ name: "pkg", version: "1.0.0" }),
    "node_modules/pkg/index.js": "export const value = 'before';"
  });
}

async function runExistingCompiler(compiler: ReturnType<typeof unpack>) {
  return new Promise<{ err: Error | null; stats?: Stats }>(
    (resolve) => {
      compiler.run((err, stats) => {
        resolve({ err, stats });
      });
    }
  );
}

async function closeCompiler(compiler: ReturnType<typeof unpack>) {
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

async function closeCompilerResult(compiler: ReturnType<typeof unpack>) {
  return new Promise<Error | null>((resolve) => {
    compiler.close((err) => {
      resolve(err);
    });
  });
}

async function closeWatching(watching: ReturnType<ReturnType<typeof unpack>["watch"]>) {
  await new Promise<void>((resolve, reject) => {
    watching.close((err) => {
      if (err) {
        reject(err);
      } else {
        resolve();
      }
    });
  });
}

function collectWatchResults() {
  const resolvers: Array<(result: { err: Error | null; stats?: Stats }) => void> = [];
  let calls = 0;
  return {
    handler: (err: Error | null, stats?: Stats) => {
      calls += 1;
      resolvers.shift()?.({ err, stats });
    },
    next: () =>
      new Promise<{ err: Error | null; stats?: Stats }>((resolve) => {
        resolvers.push(resolve);
      }),
    calls: () => calls
  };
}

function delay(ms: number) {
  return new Promise<void>((resolve) => {
    setTimeout(resolve, ms);
  });
}

type ConsoleMethod = "error" | "warn" | "info" | "log";

function captureConsole() {
  const methods: ConsoleMethod[] = ["error", "warn", "info", "log"];
  const original = Object.fromEntries(
    methods.map((method) => [method, console[method]])
  ) as Record<ConsoleMethod, (...data: unknown[]) => void>;
  const calls: Record<ConsoleMethod, string[]> = {
    error: [],
    warn: [],
    info: [],
    log: []
  };

  for (const method of methods) {
    console[method] = (...data: unknown[]) => {
      calls[method].push(data.map(String).join(" "));
    };
  }

  return {
    calls,
    all: () => methods.flatMap((method) => calls[method]),
    restore: () => {
      for (const method of methods) {
        console[method] = original[method];
      }
    }
  };
}

async function createFixture(files: Record<string, string>) {
  const root = await mkdtemp(join(tmpdir(), "unpack-api-"));
  await Promise.all(
    Object.entries(files).map(async ([path, source]) => {
      const file = join(root, path);
      await mkdir(dirname(file), { recursive: true });
      await writeFile(file, source, { encoding: "utf8" });
    })
  );
  return root;
}
