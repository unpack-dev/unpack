import { mkdir, mkdtemp, readFile, realpath, rm, stat, utimes, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

import unpack from "@unpack-js/core";
import webpack from "webpack";
import type { Compiler, Stats } from "@unpack-js/core";

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

// Ported from webpack 5.108.1's optimization side-effects cases: disabling
// optimization.sideEffects must preserve evaluation of unused package modules.
test("optimization.sideEffects false preserves unused side-effect-free modules", async () => {
  const fixture = await createFixture({
    "package.json": JSON.stringify({ sideEffects: false }),
    "src/index.js": "import { used } from './barrel'; export const result = used;",
    "src/barrel.js": "export { used } from './used'; export { unused } from './unused';",
    "src/used.js": "export const used = 42;",
    "src/unused.js": "globalThis.__unused_module_evaluated__ = true; export const unused = 0;"
  });
  const outputPath = join(fixture, "dist");

  try {
    const { err, stats } = await runCompiler({
      context: fixture,
      entry: "./src/index.js",
      output: { path: outputPath },
      sourcemap: false,
      optimization: { sideEffects: false }
    });

    assert.equal(err, null);
    assert.equal(stats?.hasErrors(), false);
    assert.match(await readFile(join(outputPath, "main.js"), "utf8"), /__unused_module_evaluated__/);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

// Ported from webpack 5.108.1 SideEffectsFlagPlugin's `analyseSource` split.
test("optimization.sideEffects distinguishes source analysis from flag-only mode", async () => {
  const fixture = await createFixture({
    "src/index.js": "import { used } from './barrel'; export const result = used;",
    "src/barrel.js": "export { used } from './used'; export { unused } from './unused';",
    "src/used.js": "export const used = 42;",
    "src/unused.js": [
      "const deferred = () => globalThis.DEFERRED_FUNCTION_BODY;",
      "class Deferred { method() { return globalThis.DEFERRED_CLASS_METHOD; } }",
      "export const UNUSED_SOURCE_ANALYSIS_MARKER = [deferred, Deferred];",
      "export const unused = 0;"
    ].join("\n")
  });

  try {
    for (const [sideEffects, markerExpected] of [[true, false], ["flag", true]] as const) {
      const outputPath = join(fixture, `dist-${sideEffects}`);
      const { err, stats } = await runCompiler({
        context: fixture,
        mode: "none",
        entry: "./src/index.js",
        output: { path: outputPath },
        sourcemap: false,
        optimization: { usedExports: true, sideEffects }
      });
      assert.equal(err, null);
      assert.equal(stats?.hasErrors(), false);
      const source = await readFile(join(outputPath, "main.js"), "utf8");
      assert.equal(source.includes("UNUSED_SOURCE_ANALYSIS_MARKER"), markerExpected);
    }
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("filesystem module cache isolates parser analysis plans", async () => {
  const fixture = await createFixture({
    "src/index.js": "import { used } from './barrel'; export const result = used;",
    "src/barrel.js": "export { used } from './used'; export { unused } from './unused';",
    "src/used.js": "export const used = 42;",
    "src/unused.js": [
      "const deferred = () => globalThis.DEFERRED_CACHE_MARKER;",
      "export const unused = deferred;"
    ].join("\n")
  });
  const cacheLocation = join(fixture, ".cache/unpack/parser-plans");

  try {
    for (const [sideEffects, markerExpected] of [["flag", true], [true, false]] as const) {
      const compiler = unpack({
        context: fixture,
        mode: "none",
        entry: "./src/index.js",
        output: { path: join(fixture, `dist-cache-${sideEffects}`) },
        cache: { type: "filesystem", cacheLocation },
        sourcemap: false,
        optimization: { usedExports: true, sideEffects }
      });
      const { err, stats } = await runExistingCompiler(compiler);
      await closeCompiler(compiler);

      assert.equal(err, null);
      assert.equal(stats?.hasErrors(), false);
      const source = await readFile(join(fixture, `dist-cache-${sideEffects}/main.js`), "utf8");
      assert.equal(source.includes("DEFERRED_CACHE_MARKER"), markerExpected);
    }
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

// Ported from webpack 5.108.1:
// test/configCases/side-effects/side-effects-values
test("package sideEffects patterns retain matching modules and skip other unused modules", async () => {
  const fixture = await createFixture({
    "package.json": JSON.stringify({ sideEffects: ["./src/kept.js"] }),
    "src/index.js": "import { used } from './barrel'; export const result = used;",
    "src/barrel.js": [
      "export { used } from './used';",
      "export { kept } from './kept';",
      "export { dropped } from './dropped';"
    ].join("\n"),
    "src/used.js": "export const used = 42;",
    "src/kept.js": "globalThis.KEPT_PATTERN_MARKER = true; export const kept = 1;",
    "src/dropped.js": "globalThis.DROPPED_PATTERN_MARKER = true; export const dropped = 2;"
  });
  const outputPath = join(fixture, "dist");

  try {
    const { err, stats } = await runCompiler({
      context: fixture,
      mode: "none",
      entry: "./src/index.js",
      output: { path: outputPath },
      sourcemap: false,
      optimization: { usedExports: true, sideEffects: "flag" }
    });
    assert.equal(err, null);
    assert.equal(stats?.hasErrors(), false);
    const source = await readFile(join(outputPath, "main.js"), "utf8");
    assert.match(source, /KEPT_PATTERN_MARKER/);
    assert.doesNotMatch(source, /DROPPED_PATTERN_MARKER/);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

// Ported from webpack 5.108.1:
// test/configCases/side-effects/no-side-effects-annotation
test("NO_SIDE_EFFECTS annotations make calls to annotated functions removable", async () => {
  const fixture = await createFixture({
    "src/index.js": "import './pure'; export const result = 42;",
    "src/pure.js": [
      "/*#__NO_SIDE_EFFECTS__*/ function fn1(value) { return value; }",
      "/*@__NO_SIDE_EFFECTS__*/ const fn2 = value => value;",
      "var fn3 = /*@__NO_SIDE_EFFECTS__*/ value => value;",
      "fn1(1); fn2(2); fn3(3);",
      "export const ANNOTATED_PURE_MODULE_MARKER = true;"
    ].join("\n")
  });
  const outputPath = join(fixture, "dist");

  try {
    const { err, stats } = await runCompiler({
      context: fixture,
      entry: "./src/index.js",
      output: { path: outputPath },
      sourcemap: false,
      optimization: { usedExports: true, sideEffects: true }
    });
    assert.equal(err, null);
    assert.equal(stats?.hasErrors(), false);
    assert.doesNotMatch(
      await readFile(join(outputPath, "main.js"), "utf8"),
      /ANNOTATED_PURE_MODULE_MARKER/
    );
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

// Compared with webpack 5.108.1's lib/javascript/JavascriptParser.js `isPure`
// behavior as consumed by lib/optimize/SideEffectsFlagPlugin.js.
test("PURE analysis skips definition-time-pure modules and retains impure arguments", async () => {
  const fixture = await createFixture({
    "src/index.js": "import { used } from './barrel'; export const result = used;",
    "src/barrel.js": [
      "export { used } from './used';",
      "export { pure } from './pure';",
      "export { impure } from './impure';"
    ].join("\n"),
    "src/used.js": "export const used = 42;",
    "src/pure.js": [
      "function factory(value) { return value; }",
      "const value = /*#__PURE__*/ factory(1);",
      "class Deferred { field = sideEffect(); method() { sideEffect(); } }",
      "export const PURE_ANALYSIS_DROP_MARKER = [value, Deferred];",
      "export const pure = 1;"
    ].join("\n"),
    "src/impure.js": [
      "function factory(value) { return value; }",
      "function sideEffect() { return 1; }",
      "const value = /*#__PURE__*/ factory(sideEffect());",
      "export const PURE_ANALYSIS_KEEP_MARKER = value;",
      "export const impure = 1;"
    ].join("\n")
  });
  const unpackOutputPath = join(fixture, "dist-unpack");

  try {
    const { err, stats } = await runCompiler({
      context: fixture,
      mode: "none",
      entry: "./src/index.js",
      output: { path: unpackOutputPath },
      sourcemap: false,
      optimization: { usedExports: true, sideEffects: true }
    });
    assert.equal(err, null);
    assert.equal(stats?.hasErrors(), false);
    const unpackSource = await readFile(join(unpackOutputPath, "main.js"), "utf8");
    const unpackObservation = {
      droppedPureModule: !unpackSource.includes("PURE_ANALYSIS_DROP_MARKER"),
      retainedImpureModule: unpackSource.includes("PURE_ANALYSIS_KEEP_MARKER")
    };

    const webpackOutputPath = join(fixture, "dist-webpack");
    const webpackCompiler = webpack({
      context: fixture,
      mode: "none",
      entry: "./src/index.js",
      output: { path: webpackOutputPath },
      devtool: false,
      optimization: {
        concatenateModules: false,
        innerGraph: false,
        minimize: false,
        providedExports: true,
        usedExports: true,
        sideEffects: true
      }
    });
    try {
      const webpackStats = await new Promise<import("webpack").Stats>((resolve, reject) => {
        webpackCompiler.run((error, completedStats) => {
          if (error) reject(error);
          else if (!completedStats) reject(new Error("webpack completed without Stats"));
          else resolve(completedStats);
        });
      });
      assert.equal(webpackStats.hasErrors(), false, webpackStats.toString());
      const webpackSource = await readFile(join(webpackOutputPath, "main.js"), "utf8");
      const webpackObservation = {
        droppedPureModule: !webpackSource.includes("PURE_ANALYSIS_DROP_MARKER"),
        retainedImpureModule: webpackSource.includes("PURE_ANALYSIS_KEEP_MARKER")
      };
      assert.deepEqual(unpackObservation, webpackObservation);
      assert.deepEqual(unpackObservation, {
        droppedPureModule: true,
        retainedImpureModule: true
      });
    } finally {
      await new Promise<void>((resolve, reject) => {
        webpackCompiler.close((error) => (error ? reject(error) : resolve()));
      });
    }
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

// Ported from webpack 5.108.1:
// test/cases/optimize/side-effects-all-chain-unused
test("side-effect-free re-export chains redirect imports to the providing module", async () => {
  const fixture = await createFixture({
    "package.json": JSON.stringify({ sideEffects: false }),
    "src/index.js": "import { value } from './barrel-a'; export const result = value;",
    "src/barrel-a.js": [
      "globalThis.BARREL_A_MARKER = true;",
      "export { value } from './barrel-b';"
    ].join("\n"),
    "src/barrel-b.js": [
      "globalThis.BARREL_B_MARKER = true;",
      "export { value } from './leaf';"
    ].join("\n"),
    "src/leaf.js": "export const value = 42;"
  });
  const outputPath = join(fixture, "dist");

  try {
    const { err, stats } = await runCompiler({
      context: fixture,
      entry: "./src/index.js",
      output: { path: outputPath },
      sourcemap: false
    });
    assert.equal(err, null);
    assert.equal(stats?.hasErrors(), false);
    const source = await readFile(join(outputPath, "main.js"), "utf8");
    assert.doesNotMatch(source, /BARREL_A_MARKER/);
    assert.doesNotMatch(source, /BARREL_B_MARKER/);
    assert.match(source, /const value = 42/);
    const entry = (await import(`${join(outputPath, "main.js")}?tree-shaking`)).default as {
      result: number;
    };
    assert.equal(entry.result, 42);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

// Ported from webpack 5.108.1:
// test/configCases/side-effects/side-effects-override
test("module rule sideEffects overrides package sideEffects metadata", async () => {
  const fixture = await createFixture({
    "package.json": JSON.stringify({ sideEffects: false }),
    "pass-through.cjs": "module.exports = source => source;",
    "src/index.js": "import { used } from './barrel'; export const result = used;",
    "src/barrel.js": "export { used } from './used'; export { unused } from './unused';",
    "src/used.js": "export const used = 42;",
    "src/unused.js": "globalThis.RULE_SIDE_EFFECTS_MARKER = true; export const unused = 0;"
  });
  const outputPath = join(fixture, "dist");

  try {
    const { err, stats } = await runCompiler({
      context: fixture,
      entry: "./src/index.js",
      output: { path: outputPath },
      sourcemap: false,
      module: {
        rules: [{
          test: /unused\.js$/,
          loader: join(fixture, "pass-through.cjs"),
          sideEffects: true
        }]
      }
    });
    assert.equal(err, null);
    assert.equal(stats?.hasErrors(), false);
    assert.match(await readFile(join(outputPath, "main.js"), "utf8"), /RULE_SIDE_EFFECTS_MARKER/);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("Stats.toJson returns an isolated baseline snapshot", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 42;"
  });

  try {
    const { err, stats } = await runCompiler({
      context: fixture,
      entry: "./src/index.js"
    });

    assert.equal(err, null);
    assert.ok(stats);
    const expected = stats.toJson();
    const mutated = stats.toJson();
    assert.notStrictEqual(mutated, expected);
    assert.notStrictEqual(mutated.assets, expected.assets);
    assert.notStrictEqual(mutated.errors, expected.errors);
    assert.notStrictEqual(mutated.warnings, expected.warnings);
    assert.notStrictEqual(mutated.watchDependencies, expected.watchDependencies);

    const firstAsset = mutated.assets[0];
    assert.ok(firstAsset);
    firstAsset.name = "changed.js";
    mutated.errors.push({ message: "changed error" });
    mutated.warnings.push({ message: "changed warning" });
    mutated.watchDependencies.files.push("changed dependency");

    assert.deepEqual(stats.toJson(), expected);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

// Ported from webpack 5.108.1's DefinePropertyGettersRuntimeModule,
// HasOwnPropertyRuntimeModule, and MakeNamespaceObjectRuntimeModule behavior.
test("static ESM emits only the runtime capabilities used by generated code", async () => {
  const fixture = await createFixture({
    "src/index.js": "import { value } from './dep'; export const result = value;",
    "src/dep.js": "export const value = 42;"
  });
  const outputPath = join(fixture, "dist");

  try {
    const { err, stats } = await runCompiler({
      context: fixture,
      entry: "./src/index.js",
      output: { path: outputPath },
      sourcemap: false
    });

    assert.equal(err, null);
    assert.equal(stats?.hasErrors(), false);
    const source = await readFile(join(outputPath, "main.js"), "utf8");
    assert.match(source, /__webpack_require__\.d =/);
    assert.match(source, /__webpack_require__\.o =/);
    assert.match(source, /__webpack_require__\.r =/);
    assert.doesNotMatch(source, /__webpack_require__\.e =/);
    assert.doesNotMatch(source, /__webpack_require__\.f =/);
    assert.doesNotMatch(source, /__webpack_require__\.u =/);
    assert.doesNotMatch(source, /installedChunks/);
    assert.doesNotMatch(source, /installChunk/);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

// Ported from webpack 5.108.1's HarmonyCompatibilityDependency behavior.
test("namespace compatibility marks static ESM but not scripts or dynamic-import-only modules", async () => {
  const scriptFixture = await createFixture({
    "src/index.js": "globalThis.__unpack_script__ = 1;"
  });
  const dynamicFixture = await createFixture({
    "src/index.js": "globalThis.__unpack_load__ = () => import('./feature');",
    "src/feature.js": "export const value = 42;"
  });

  try {
    const script = await runCompiler({
      context: scriptFixture,
      entry: "./src/index.js",
      sourcemap: false
    });
    assert.equal(script.err, null);
    assert.equal(script.stats?.hasErrors(), false);
    const scriptSource = await readFile(join(scriptFixture, "dist/main.js"), "utf8");
    assert.doesNotMatch(scriptSource, /__webpack_require__\.r\(__webpack_exports__\)/);
    assert.doesNotMatch(scriptSource, /__webpack_require__\.r =/);

    const dynamic = await runCompiler({
      context: dynamicFixture,
      entry: "./src/index.js",
      sourcemap: false
    });
    assert.equal(dynamic.err, null);
    assert.equal(dynamic.stats?.hasErrors(), false);
    const dynamicAssets = dynamic.stats?.toJson().assets.map((asset) => asset.name) ?? [];
    const asyncAsset = dynamicAssets.find((asset) => asset !== "main.js");
    assert.ok(asyncAsset);
    const mainSource = await readFile(join(dynamicFixture, "dist/main.js"), "utf8");
    const asyncSource = await readFile(join(dynamicFixture, "dist", asyncAsset), "utf8");
    assert.doesNotMatch(mainSource, /__webpack_require__\.r\(__webpack_exports__\)/);
    assert.match(asyncSource, /__webpack_require__\.r\(__webpack_exports__\)/);
  } finally {
    await rm(scriptFixture, { recursive: true, force: true });
    await rm(dynamicFixture, { recursive: true, force: true });
  }
});

// Ported from webpack 5.108.1's collapsed import() behavior when a target is
// already available in the initial chunk.
test("collapsed dynamic imports do not provision asynchronous runtime capabilities", async () => {
  const fixture = await createFixture({
    "src/index.js": [
      "import { value } from './dep';",
      "export const initial = value;",
      "export const load = () => import('./dep');"
    ].join("\n"),
    "src/dep.js": "export const value = 42;"
  });

  try {
    const { err, stats } = await runCompiler({
      context: fixture,
      entry: "./src/index.js",
      sourcemap: false
    });

    assert.equal(err, null);
    assert.equal(stats?.hasErrors(), false);
    assert.deepEqual(stats?.toJson().assets.map((asset) => asset.name), ["main.js"]);
    const source = await readFile(join(fixture, "dist/main.js"), "utf8");
    assert.match(source, /Promise\.resolve\(\)\.then/);
    assert.doesNotMatch(source, /__webpack_require__\.e =/);
    assert.doesNotMatch(source, /__webpack_require__\.f =/);
    assert.doesNotMatch(source, /__webpack_require__\.u =/);
    assert.doesNotMatch(source, /installedChunks/);
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

test("can disable sourcemap emission", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 42;"
  });

  try {
    const { err, stats } = await runCompiler({
      context: fixture,
      entry: "./src/index.js",
      sourcemap: false
    });

    assert.equal(err, null);
    assert.ok(stats);
    assert.deepEqual(
      stats.toJson().assets.map((asset) => asset.name).sort(),
      ["main.js"]
    );
    const main = await readFile(join(fixture, "dist/main.js"), "utf8");
    await assert.rejects(readFile(join(fixture, "dist/main.js.map"), "utf8"));
    assert.doesNotMatch(main, /sourceMappingURL/);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("plugins apply once in configuration order and run on every compilation", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const events: string[] = [];
  const objectPlugin = {
    apply(compiler: Compiler) {
      events.push("object:apply");
      compiler.hooks.done.tap("object plugin", () => events.push("object:done"));
    }
  };
  function functionPlugin(this: Compiler, compiler: Compiler): void {
    assert.equal(this, compiler);
    events.push("function:apply");
    compiler.hooks.done.tap("function plugin", () => events.push("function:done"));
  }
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    sourcemap: false,
    plugins: [false, objectPlugin, null, functionPlugin, 0, "", undefined]
  });

  try {
    assert.deepEqual(events, ["object:apply", "function:apply"]);
    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.deepEqual(events, [
      "object:apply",
      "function:apply",
      "object:done",
      "function:done",
      "object:done",
      "function:done"
    ]);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("plugin application failures follow top-level initialization error timing", async () => {
  const synchronousFailure = new Error("synchronous plugin failure");
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        plugins: [{ apply: () => { throw synchronousFailure; } }]
      }),
    (error) => error === synchronousFailure
  );

  const asynchronousFailure = new Error("callback plugin failure");
  let calledSynchronously = true;
  const observation = new Promise<{ calledSynchronously: boolean; error: Error | null }>(
    (resolve) => {
      const returnedCompiler = unpack(
        {
          entry: "./src/index.js",
          plugins: [{ apply: () => { throw asynchronousFailure; } }]
        },
        (error) => resolve({ calledSynchronously, error })
      );
      assert.equal(returnedCompiler, null);
      calledSynchronously = false;
    }
  );

  assert.deepEqual(await observation, {
    calledSynchronously: false,
    error: asynchronousFailure
  });
  assert.equal(asynchronousFailure.name, "InfrastructureError");
});

test("unpack options callback returns a reusable compiler", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });

  try {
    let compiler!: ReturnType<typeof unpack>;
    const callbackResult = new Promise<{ err: Error | null; stats?: Stats }>(
      (resolve) => {
        const returnedCompiler = unpack(
          {
            context: fixture,
            entry: "./src/index.js"
          },
          (err, stats) => resolve({ err, stats })
        );
        assert.ok(returnedCompiler);
        compiler = returnedCompiler;
      }
    );

    const { err, stats } = await callbackResult;
    assert.equal(err, null);
    assert.equal(stats?.hasErrors(), false);
    const rerun = await runExistingCompiler(compiler);
    assert.equal(rerun.err, null);
    assert.equal(rerun.stats?.hasErrors(), false);
    await closeCompiler(compiler);
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

test("omitted and production mode default resolve snapshots to timestamp plus hash", async () => {
  await assertSameTimestampPackageExportsEditEmits("after", {});
  await assertSameTimestampPackageExportsEditEmits("after", { mode: "production" });
});

test("development and none default resolve snapshots to timestamp only", async () => {
  await assertSameTimestampPackageExportsEditEmits("before", { mode: "development" });
  await assertSameTimestampPackageExportsEditEmits("before", { mode: "none" });
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

test("unmanaged path patterns make resolver missing candidates invalidate managed packages", async () => {
  const fixture = await createFixture({
    "src/index.js": "import { value } from 'pkg/feature'; export const result = value;",
    "node_modules/pkg/package.json": JSON.stringify({ name: "pkg", version: "1.0.0" }),
    "node_modules/pkg/feature.js": "export const value = 'js';"
  });
  const packageRoot = join(fixture, "node_modules/pkg");
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: true,
    snapshot: {
      resolve: { timestamp: true, hash: false },
      unmanagedPaths: [packageRoot]
    }
  });

  try {
    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /js/);

    await writeFile(join(packageRoot, "feature.ts"), "export const value = 'ts';", {
      encoding: "utf8"
    });

    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /ts/);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("missing resolver candidates appearing invalidate resolve records", async () => {
  const fixture = await createFixture({
    "src/index.js": "import { value } from './dep'; export const result = value;",
    "src/dep.js": "export const value = 'js';"
  });
  const compiler = unpack({
    context: fixture,
    mode: "development",
    entry: "./src/index.js",
    cache: true
  });

  try {
    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /js/);

    await writeFile(join(fixture, "src/dep.ts"), "export const value = 'ts';", {
      encoding: "utf8"
    });

    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /ts/);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("context directory candidate changes invalidate resolve records", async () => {
  const fixture = await createFixture({
    "src/index.js": "import { value } from './pkg'; export const result = value;",
    "src/pkg/package.json": '{"main":"before.js"}',
    "src/pkg/before.js": "export const value = 'before';",
    "src/pkg/after.js": "export const value = 'after';"
  });
  const packageJson = join(fixture, "src/pkg/package.json");
  const stableTime = new Date("2020-01-01T00:00:00.000Z");
  await utimes(packageJson, stableTime, stableTime);
  const compiler = unpack({
    context: fixture,
    mode: "development",
    entry: "./src/index.js",
    cache: true
  });

  try {
    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /before/);

    await writeFile(packageJson, '{"main":"after.js"}', { encoding: "utf8" });
    await writeFile(join(fixture, "src/pkg/unused.js"), "export const unused = true;", {
      encoding: "utf8"
    });
    await utimes(packageJson, stableTime, stableTime);

    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /after/);
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
    cacheDirectory: join(fixture, ".cache/unpack"),
    name: "test-cache",
    version: "v1",
    buildDependencies: {
      config: ["./config/build.js"]
    },
    idleTimeout: 10,
    readonly: false
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
    assert.ok(
      await stat(
        join(fixture, ".cache/unpack/test-cache/turbo-persistence/CURRENT")
      )
    );
    await assert.rejects(
      stat(join(fixture, ".cache/unpack/test-cache/container.json"))
    );
    await assert.rejects(
      stat(join(fixture, ".cache/unpack/test-cache/packs/modules.cbor"))
    );

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

test("filesystem cache flushes after the initial-store timeout", async () => {
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
      idleTimeout: 60_000,
      idleTimeoutForInitialStore: 30
    }
  });

  try {
    const result = await runExistingCompiler(compiler);
    assert.equal(result.err, null);
    await assert.rejects(
      stat(join(cacheLocation, "turbo-persistence/CURRENT"))
    );

    await waitForObservation(
      () => stat(join(cacheLocation, "turbo-persistence/CURRENT")),
      () => true,
      "initial persistent cache publication"
    );
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("repeated run uses the ordinary idle cache timeout", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 'before';"
  });
  const entry = join(fixture, "src/index.js");
  const cacheLocation = join(fixture, ".cache/unpack/ordinary-idle");
  const currentPath = join(cacheLocation, "turbo-persistence/CURRENT");
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: {
      type: "filesystem",
      cacheLocation,
      idleTimeout: 150,
      idleTimeoutForInitialStore: 0,
      idleTimeoutAfterLargeChanges: 10
    }
  });

  try {
    assert.equal((await runExistingCompiler(compiler)).err, null);
    const firstRevision = await waitForObservation(
      () => readTurboPersistenceSequence(currentPath),
      (revision) => revision > 0,
      "initial turbo-persistence revision"
    );

    await writeFile(entry, "export const value = 'after';", "utf8");
    const changedTime = new Date(Date.now() + 2000);
    await utimes(entry, changedTime, changedTime);
    assert.equal((await runExistingCompiler(compiler)).err, null);

    await delay(60);
    assert.equal(await readTurboPersistenceSequence(currentPath), firstRevision);
    const nextRevision = await waitForObservation(
      () => readTurboPersistenceSequence(currentPath),
      (revision) => revision > firstRevision,
      "ordinary-idle turbo-persistence revision"
    );
    assert.ok(nextRevision > firstRevision);
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
    assert.ok(await stat(join(cacheLocation, "turbo-persistence/CURRENT")));
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("run and watch started while the compiler is closing fail deterministically", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: {
      type: "filesystem",
      cacheLocation: join(fixture, ".cache/unpack/closing"),
      idleTimeout: 60_000
    }
  });

  try {
    assert.equal((await runExistingCompiler(compiler)).err, null);

    const closeResult = closeCompilerResult(compiler);
    const runResultPromise = runExistingCompiler(compiler);
    const watchResults = collectWatchResults();
    const watchResultPromise = watchResults.next();
    const watching = compiler.watch({}, watchResults.handler);
    const [runResult, watchResult] = await Promise.all([
      runResultPromise,
      watchResultPromise
    ]);

    assert.equal(runResult.err?.name, "CompilerClosedError");
    assert.equal(runResult.stats, undefined);
    assert.equal(watchResult.err?.name, "CompilerClosedError");
    assert.equal(watchResult.stats, undefined);
    await closeWatching(watching);
    assert.equal(await closeResult, null);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("close coalesces concurrent callers and remains asynchronous and idempotent", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: {
      type: "filesystem",
      cacheLocation: join(fixture, ".cache/unpack/coalesced-close"),
      idleTimeout: 60_000
    }
  });

  try {
    assert.equal((await runExistingCompiler(compiler)).err, null);

    const [first, second] = await Promise.all([
      observeCompilerClose(compiler),
      observeCompilerClose(compiler)
    ]);
    for (const observation of [first, second]) {
      assert.equal(observation.calledSynchronously, false);
      assert.equal(observation.calls, 1);
      assert.equal(observation.err, null);
    }

    const repeated = await observeCompilerClose(compiler);
    assert.equal(repeated.calledSynchronously, false);
    assert.equal(repeated.calls, 1);
    assert.equal(repeated.err, null);
    assert.equal((await runExistingCompiler(compiler)).err?.name, "CompilerClosedError");
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("filesystem cache publication failures are warnings and do not fail close", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;",
    ".cache/unpack/unwritable": "not a directory"
  });
  const captured = captureConsole();
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: {
      type: "filesystem",
      cacheLocation: join(fixture, ".cache/unpack/unwritable"),
      idleTimeout: 60_000
    },
    infrastructureLogging: { level: "warn" }
  });

  try {
    const result = await runExistingCompiler(compiler);
    assert.equal(result.err, null);

    assert.equal(await closeCompilerResult(compiler), null);
    assert.ok(captured.calls.warn.some((event) => event.startsWith("[unpack.Cache]")));
  } finally {
    captured.restore();
    await rm(fixture, { recursive: true, force: true });
  }
});

test("filesystem cache publication failures do not fail watching close", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;",
    ".cache/unpack/unwritable": "not a directory"
  });
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: {
      type: "filesystem",
      cacheLocation: join(fixture, ".cache/unpack/unwritable"),
      idleTimeout: 60_000
    }
  });

  try {
    const results = collectWatchResults();
    const first = results.next();
    const watching = compiler.watch({}, results.handler);
    assert.equal((await first).err, null);

    await closeWatching(watching);
    await closeCompiler(compiler);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("filesystem cache readonly skips persistent writes", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const cacheLocation = join(fixture, ".cache/unpack/readonly");
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: {
      type: "filesystem",
      cacheLocation,
      idleTimeout: 0,
      readonly: true
    }
  });

  try {
    const result = await runExistingCompiler(compiler);
    assert.equal(result.err, null);
    await delay(50);

    await closeCompiler(compiler);
    await assert.rejects(stat(cacheLocation));
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

test("watch close settles pending filesystem cache work and keeps compiler reusable", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const cacheLocation = join(fixture, ".cache/unpack/watch-close");
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: {
      type: "filesystem",
      cacheLocation,
      idleTimeoutForInitialStore: 60_000
    }
  });

  try {
    const results = collectWatchResults();
    const first = results.next();
    const watching = compiler.watch({}, results.handler);
    assert.equal((await first).err, null);
    await assert.rejects(
      stat(join(cacheLocation, "turbo-persistence/CURRENT"))
    );

    await closeWatching(watching);
    assert.ok(await stat(join(cacheLocation, "turbo-persistence/CURRENT")));
    assert.equal((await runExistingCompiler(compiler)).err, null);
    await closeCompiler(compiler);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("serial rebuild Make preserves watch invalidation behavior", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 'before';"
  });
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js"
  });
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

test("watch invalidation uses the post-large-change cache timeout", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 'before';"
  });
  const entry = join(fixture, "src/index.js");
  const cacheLocation = join(fixture, ".cache/unpack/watch-large-change");
  const currentPath = join(cacheLocation, "turbo-persistence/CURRENT");
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: {
      type: "filesystem",
      cacheLocation,
      idleTimeout: 60_000,
      idleTimeoutForInitialStore: 0,
      idleTimeoutAfterLargeChanges: 150
    }
  });

  try {
    const results = collectWatchResults();
    const first = results.next();
    const watching = compiler.watch({}, results.handler);
    assert.equal((await first).err, null);
    const firstRevision = await waitForObservation(
      () => readTurboPersistenceSequence(currentPath),
      (revision) => revision > 0,
      "initial watch turbo-persistence revision"
    );

    const second = results.next();
    await writeFile(entry, "export const value = 'after';", "utf8");
    const changedTime = new Date(Date.now() + 2000);
    await utimes(entry, changedTime, changedTime);
    watching.invalidate();
    assert.equal((await second).err, null);

    await delay(60);
    assert.equal(await readTurboPersistenceSequence(currentPath), firstRevision);
    await delay(180);
    assert.ok(
      (await readTurboPersistenceSequence(currentPath)) > firstRevision
    );
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

test("unsafe watch cache invalidation rebuilds changed files", async () => {
  const fixture = await createFixture({
    "src/index.js":
      "import { changed } from './changed'; import { stable } from './stable'; export const result = `${changed}:${stable}`;",
    "src/changed.js": "export const changed = 'before';",
    "src/stable.js": "export const stable = 'stable';"
  });
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    experiments: { unsafeWatchCacheInvalidation: true }
  });
  const dependency = join(fixture, "src/changed.js");

  try {
    const results = collectWatchResults();
    const first = results.next();
    const watching = compiler.watch({}, results.handler);
    assert.equal((await first).err, null);

    const second = results.next();
    await writeFile(dependency, "export const changed = 'after';", "utf8");
    const changedTime = new Date(Date.now() + 2000);
    await utimes(dependency, changedTime, changedTime);

    assert.equal((await second).err, null);
    const bundle = await readFile(join(fixture, "dist/main.js"), "utf8");
    assert.match(bundle, /after/);
    assert.match(bundle, /stable/);
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

test("cache profile reports persistent activity through infrastructure logging only", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const captured = captureConsole();
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: {
      type: "filesystem",
      cacheLocation: join(fixture, ".cache/unpack/profile"),
      profile: true,
      idleTimeout: 0
    },
    infrastructureLogging: { level: "log" }
  });

  try {
    const result = await runExistingCompiler(compiler);
    await closeCompiler(compiler);

    assert.equal(result.err, null);
    assert.ok(result.stats);
    assert.equal("cache" in result.stats.toJson(), false);
    assert.equal("logs" in result.stats.toJson(), false);
    const profile = captured.calls.log.filter((message) =>
      message.startsWith("[unpack.Cache.Profile]")
    );
    for (const activity of [
      "restore",
      "store",
      "serialization",
      "deserialization",
      "garbage collection",
      "turbo-persistence transaction",
      "compaction"
    ]) {
      assert.ok(profile.some((message) => message.includes(activity)), activity);
    }
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
        sourcemap: "hidden"
      }),
    /options.sourcemap must be a boolean/
  );
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        // @ts-expect-error intentionally testing runtime validation
        plugins: {}
      }),
    /options.plugins must be an array/
  );
  assert.throws(
    () =>
      unpack({
        entry: "./src/index.js",
        // @ts-expect-error intentionally testing runtime validation
        plugins: [{}]
      }),
    /options.plugins\[0\] must be a function, a plugin with an apply method, or falsy/
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
        cache: {
          type: "filesystem",
          // @ts-expect-error intentionally testing runtime validation
          readonly: "yes"
        }
      }),
    /options.cache.readonly must be a boolean/
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
  assert.doesNotThrow(() =>
    unpack({
      entry: "./src/index.js",
      snapshot: {
        module: {
          timestamp: false,
          hash: false
        }
      }
    })
  );
  assert.doesNotThrow(() =>
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
    })
  );
  assert.doesNotThrow(() =>
    unpack({
      entry: "./src/index.js",
      snapshot: {
        resolveBuildDependencies: {
          timestamp: false,
          hash: false
        }
      }
    })
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
  const compiler = unpack(options);
  try {
    return await runExistingCompiler(compiler);
  } finally {
    await closeCompiler(compiler);
  }
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

async function assertSameTimestampPackageExportsEditEmits(
  expected: string,
  options: Partial<Parameters<typeof unpack>[0]>
) {
  const fixture = await createFixture({
    "src/index.js": "import { value } from 'pkg/feature'; export const result = value;",
    "node_modules/pkg/package.json": JSON.stringify({
      name: "pkg",
      version: "1.0.0",
      exports: { "./feature": "./before.js" }
    }),
    "node_modules/pkg/before.js": "export const value = 'before';",
    "node_modules/pkg/after.js": "export const value = 'after';"
  });
  const packageRoot = join(fixture, "node_modules/pkg");
  const packageJson = join(packageRoot, "package.json");
  const stableTime = new Date("2020-01-01T00:00:00.000Z");
  await utimes(packageJson, stableTime, stableTime);
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    cache: true,
    snapshot: {
      unmanagedPaths: [packageRoot]
    },
    ...options
  });

  try {
    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /const value = 'before'/);

    await writeFile(
      packageJson,
      JSON.stringify({
        name: "pkg",
        version: "1.0.0",
        exports: { "./feature": "./after.js" }
      }),
      { encoding: "utf8" }
    );
    await utimes(packageJson, stableTime, stableTime);

    assert.equal((await runExistingCompiler(compiler)).err, null);
    assert.match(
      await readFile(join(fixture, "dist/main.js"), "utf8"),
      new RegExp(`const value = '${expected}'`)
    );
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

async function observeCompilerClose(compiler: ReturnType<typeof unpack>) {
  let calledSynchronously = true;
  let calls = 0;
  let callbackError: Error | null = null;
  const firstDeliveryWasSynchronous = await new Promise<boolean>((resolve) => {
    compiler.close((error) => {
      calls += 1;
      callbackError = error;
      if (calls === 1) {
        const synchronous = calledSynchronously;
        setTimeout(() => resolve(synchronous), 0);
      }
    });
    calledSynchronously = false;
  });
  return {
    calledSynchronously: firstDeliveryWasSynchronous,
    calls,
    err: callbackError
  };
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

async function readTurboPersistenceSequence(
  currentPath: string
): Promise<number> {
  const current = JSON.parse(await readFile(currentPath, "utf8")) as {
    max_sequence_number?: unknown;
  };
  if (typeof current.max_sequence_number !== "number") {
    throw new TypeError("invalid turbo-persistence CURRENT sequence");
  }
  return current.max_sequence_number;
}

async function waitForObservation<T>(
  observe: () => Promise<T>,
  isReady: (value: T) => boolean,
  description: string
): Promise<T> {
  const deadline = Date.now() + 10_000;
  let lastError: unknown;
  while (Date.now() < deadline) {
    try {
      const value = await observe();
      if (isReady(value)) return value;
    } catch (error) {
      lastError = error;
    }
    await delay(20);
  }

  throw new Error(`timed out waiting for ${description}`, {
    cause: lastError
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
