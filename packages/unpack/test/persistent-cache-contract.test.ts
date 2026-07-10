import { mkdir, mkdtemp, readFile, rm, stat, utimes, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import assert from "node:assert/strict";
import test from "node:test";

import unpack from "@unpack-js/core";
import type { CacheOptions, Compiler, Mode, Stats } from "@unpack-js/core";

import {
  runCacheProcess,
  runColdWarmBuilds
} from "./cache-process-harness.js";
import type { CacheProcessObservation } from "./cache-process-harness.js";

test("omitted cache follows mode while both-disabled module Snapshots remain valid configuration", async () => {
  await assertOmittedCacheBehavior("development", "before");
  await assertOmittedCacheBehavior("production", "after");
  await assertOmittedCacheBehavior("none", "after");
});

test("cache booleans override mode-dependent defaults", async () => {
  await assertCacheOverrideBehavior("production", true, "before");
  await assertCacheOverrideBehavior("development", false, "after");
});

test("cache objects require a type and reject fields outside the selected cache type synchronously", () => {
  const createCompiler = (cache: unknown) => () =>
    unpack({
      entry: "./src/index.js",
      cache: cache as CacheOptions
    });

  assert.throws(createCompiler({}), /options\.cache\.type is required/);
  assert.throws(
    createCompiler({ type: "memory", cacheLocation: join(tmpdir(), "cache") }),
    /options\.cache\.cacheLocation is only valid for filesystem cache/
  );
  assert.throws(
    createCompiler({ type: "filesystem", maxMemoryGenerations: 2 }),
    /options\.cache contains unsupported option 'maxMemoryGenerations'/
  );
  assert.throws(
    createCompiler({ type: "memory", cacheUnaffected: true }),
    /options\.cache contains unsupported option 'cacheUnaffected'/
  );
  assert.throws(
    createCompiler({ type: "filesystem", memoryCacheUnaffected: true }),
    /options\.cache contains unsupported option 'memoryCacheUnaffected'/
  );
  assert.doesNotThrow(createCompiler({ type: "memory" }));
});

test("filesystem cache derives its name and directory from the top-level contract across processes", async () => {
  const fixture = await createFixture({
    "package.json": "{}",
    "src/index.js": "export const value = 1;"
  });
  const invocationDirectory = join(fixture, "tools/invocation");
  const outputPath = join(fixture, "dist");
  await mkdir(invocationDirectory, { recursive: true });

  try {
    const { cold, warm } = await runColdWarmBuilds(
      {
        bundler: "unpack",
        options: {
          context: fixture,
          entry: "./src/index.js",
          mode: "development",
          name: "client",
          outputPath,
          cache: { type: "filesystem" }
        }
      },
      { cwd: invocationDirectory }
    );

    assert.equal(cold.error, null);
    assert.equal(warm.error, null);
    assert.notEqual(cold.pid, warm.pid);
    assert.notEqual(cold.instanceId, warm.instanceId);
    assert.deepEqual(cold.assets, ["main.js"]);
    assert.deepEqual(warm.assets, ["main.js"]);
    assert.ok(
      await stat(
        join(
          fixture,
          "node_modules/.cache/unpack/client-development"
        )
      )
    );
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("explicit filesystem cache paths must be absolute and cacheLocation takes precedence", async () => {
  const createCompiler = (cache: CacheOptions) => () =>
    unpack({ entry: "./src/index.js", cache });
  assert.throws(
    createCompiler({ type: "filesystem", cacheDirectory: ".cache/unpack" }),
    /options\.cache\.cacheDirectory must be an absolute path/
  );
  assert.throws(
    createCompiler({ type: "filesystem", cacheLocation: ".cache/unpack/app" }),
    /options\.cache\.cacheLocation must be an absolute path/
  );

  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const explicitLocation = join(fixture, "explicit-cache");
  const ignoredDirectory = join(fixture, "ignored-cache-directory");

  try {
    const { cold, warm } = await runColdWarmBuilds(
      {
        bundler: "unpack",
        options: {
          context: fixture,
          mode: "none",
          outputPath: join(fixture, "dist"),
          cache: {
            type: "filesystem",
            cacheDirectory: ignoredDirectory,
            cacheLocation: explicitLocation,
            name: "ignored-name"
          }
        }
      },
      { cwd: fixture }
    );

    assert.equal(cold.error, null);
    assert.equal(warm.error, null);
    assert.ok(await stat(explicitLocation));
    await assert.rejects(stat(join(ignoredDirectory, "ignored-name")));
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("only the approved webpack cache fields are accepted as inert", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 'inert-options';"
  });
  const cacheLocation = join(fixture, ".cache/unpack/inert");
  const createCompiler = (cache: unknown) => () =>
    unpack({
      context: fixture,
      entry: "./src/index.js",
      cache: cache as CacheOptions
    });

  try {
    assert.throws(
      createCompiler({ type: "filesystem", hashAlgorithm: 42 }),
      /options\.cache\.hashAlgorithm must be a string/
    );
    assert.throws(
      createCompiler({ type: "filesystem", managedPaths: "node_modules" }),
      /options\.cache\.managedPaths must be an array/
    );
    assert.throws(
      createCompiler({ type: "filesystem", immutablePaths: ["relative"] }),
      /options\.cache\.immutablePaths\[0\] must be an absolute path/
    );
    assert.throws(
      createCompiler({ type: "filesystem", maxAge: 1 }),
      /options\.cache contains unknown option 'maxAge'/
    );

    const compiler = unpack({
      context: fixture,
      entry: "./src/index.js",
      sourcemap: false,
      cache: {
        type: "filesystem",
        cacheLocation,
        hashAlgorithm: "not-a-runtime-hash",
        managedPaths: [fixture, /node_modules/g],
        immutablePaths: [/immutable/y]
      }
    });
    const result = await runCompiler(compiler);
    await closeCompiler(compiler);

    assert.equal(result.err, null);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /inert-options/);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("selective cold and warm observations run Unpack and pinned webpack in independent processes", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 'comparison';"
  });

  try {
    const unpackBuilds = await runColdWarmBuilds({
      bundler: "unpack",
      options: {
        context: fixture,
        mode: "development",
        outputPath: join(fixture, "dist-unpack"),
        cache: {
          type: "filesystem",
          cacheLocation: join(fixture, ".cache/unpack/comparison"),
          hashAlgorithm: "observed-inert-value",
          managedPaths: [fixture],
          immutablePaths: [fixture]
        },
        snapshot: {
          module: { timestamp: false, hash: false }
        }
      }
    });
    const webpackBuilds = await runColdWarmBuilds({
      bundler: "webpack",
      options: {
        context: fixture,
        mode: "development",
        outputPath: join(fixture, "dist-webpack"),
        cache: {
          type: "filesystem",
          cacheLocation: join(fixture, ".cache/webpack/comparison"),
          hashAlgorithm: "observed-inert-value",
          managedPaths: [fixture],
          immutablePaths: [fixture]
        },
        snapshot: {
          module: { timestamp: false, hash: false }
        }
      }
    });

    assertColdWarmPublicOutcome(unpackBuilds);
    assertColdWarmPublicOutcome(webpackBuilds);
    assert.deepEqual(
      publicBuildOutcome(unpackBuilds.cold),
      publicBuildOutcome(webpackBuilds.cold)
    );
    assert.deepEqual(
      publicBuildOutcome(unpackBuilds.warm),
      publicBuildOutcome(webpackBuilds.warm)
    );
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("cache validation is synchronous for Unpack and pinned webpack", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });
  const options = {
    context: fixture,
    outputPath: join(fixture, "dist"),
    cache: {}
  };

  try {
    const unpackObservation = await runCacheProcess({ bundler: "unpack", options });
    const webpackObservation = await runCacheProcess({ bundler: "webpack", options });

    for (const observation of [unpackObservation, webpackObservation]) {
      assert.equal(observation.synchronousError, true);
      assert.ok(observation.error);
      assert.equal(observation.hasStats, false);
    }
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("filesystem cache falls back to cwd and explicit cache name overrides top-level name", async () => {
  const fixture = await createFixture({
    "src/index.js": "export const value = 1;"
  });

  try {
    const { cold, warm } = await runColdWarmBuilds(
      {
        bundler: "unpack",
        options: {
          context: fixture,
          mode: "production",
          name: "top-level",
          outputPath: join(fixture, "dist"),
          cache: { type: "filesystem", name: "explicit-cache-name" }
        }
      },
      { cwd: fixture }
    );

    assert.equal(cold.error, null);
    assert.equal(warm.error, null);
    assert.ok(
      await stat(
        join(fixture, ".cache/unpack/explicit-cache-name")
      )
    );
    await assert.rejects(
      stat(join(fixture, ".cache/unpack/top-level-production"))
    );
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("Context Module and cache-unaffected surfaces remain synchronously unsupported", () => {
  const createCompiler = (options: Record<string, unknown>) => () =>
    unpack(
      {
        entry: "./src/index.js",
        ...options
      } as unknown as Parameters<typeof unpack>[0]
    );

  assert.throws(
    createCompiler({ snapshot: { contextModule: { timestamp: true } } }),
    /options\.snapshot contains unknown option 'contextModule'/
  );
  assert.throws(
    createCompiler({ cache: { type: "memory", cacheUnaffected: true } }),
    /options\.cache contains unsupported option 'cacheUnaffected'/
  );
  assert.throws(
    createCompiler({
      cache: { type: "filesystem", memoryCacheUnaffected: true }
    }),
    /options\.cache contains unsupported option 'memoryCacheUnaffected'/
  );
});

test("filesystem caching fails clearly in an unsupported Yarn Plug'n'Play runtime", () => {
  const versions = process.versions as NodeJS.ProcessVersions & { pnp?: string };
  const previous = versions.pnp;
  versions.pnp = "3";

  try {
    assert.throws(
      () =>
        unpack({
          entry: "./src/index.js",
          cache: {
            type: "filesystem",
            cacheLocation: join(tmpdir(), "unpack-pnp-cache")
          }
        }),
      /Yarn Plug'n'Play is not supported by filesystem cache/
    );
  } finally {
    if (previous === undefined) {
      delete versions.pnp;
    } else {
      versions.pnp = previous;
    }
  }
});

function assertColdWarmPublicOutcome(builds: {
  cold: CacheProcessObservation;
  warm: CacheProcessObservation;
}) {
  assert.notEqual(builds.cold.pid, builds.warm.pid);
  assert.notEqual(builds.cold.instanceId, builds.warm.instanceId);
  assert.deepEqual(publicBuildOutcome(builds.cold), {
    synchronousError: false,
    error: null,
    hasStats: true,
    hasErrors: false,
    assets: ["main.js"]
  });
  assert.deepEqual(publicBuildOutcome(builds.warm), publicBuildOutcome(builds.cold));
}

function publicBuildOutcome(observation: CacheProcessObservation) {
  return {
    synchronousError: observation.synchronousError,
    error: observation.error,
    hasStats: observation.hasStats,
    hasErrors: observation.hasErrors,
    assets: observation.assets
  };
}

async function assertOmittedCacheBehavior(mode: Mode, expected: "before" | "after") {
  await assertCacheBehavior(mode, expected);
}

async function assertCacheOverrideBehavior(
  mode: Mode,
  cache: boolean,
  expected: "before" | "after"
) {
  await assertCacheBehavior(mode, expected, cache);
}

async function assertCacheBehavior(
  mode: Mode,
  expected: "before" | "after",
  cache?: boolean
) {
  const fixture = await createFixture({
    "src/index.js": "export const value = 'before';"
  });
  const entry = join(fixture, "src/index.js");
  const output = join(fixture, "dist/main.js");
  const stableTime = new Date("2020-01-01T00:00:00.000Z");
  await utimes(entry, stableTime, stableTime);
  const compiler = unpack({
    context: fixture,
    mode,
    entry: "./src/index.js",
    sourcemap: false,
    ...(cache === undefined ? {} : { cache }),
    snapshot: {
      module: { timestamp: false, hash: false }
    }
  });

  try {
    assert.equal((await runCompiler(compiler)).err, null);
    assert.match(await readFile(output, "utf8"), /before/);
    await writeFile(entry, "export const value = 'after';", "utf8");
    await utimes(entry, stableTime, stableTime);
    assert.equal((await runCompiler(compiler)).err, null);
    assert.match(await readFile(output, "utf8"), new RegExp(expected));
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
}

async function runCompiler(compiler: Compiler) {
  return new Promise<{ err: Error | null; stats?: Stats }>((resolve) => {
    compiler.run((err, stats) => resolve({ err, stats }));
  });
}

async function closeCompiler(compiler: Compiler) {
  await new Promise<void>((resolve, reject) => {
    compiler.close((err) => (err ? reject(err) : resolve()));
  });
}

async function createFixture(files: Record<string, string>) {
  const root = await mkdtemp(join(tmpdir(), "unpack-cache-contract-"));
  await Promise.all(
    Object.entries(files).map(async ([path, source]) => {
      const file = join(root, path);
      await mkdir(dirname(file), { recursive: true });
      await writeFile(file, source, "utf8");
    })
  );
  return root;
}
