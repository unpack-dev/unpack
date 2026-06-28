import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
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

test("top-level option validation throws synchronously", () => {
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

async function runCompiler(options: Parameters<typeof unpack>[0]) {
  return runExistingCompiler(unpack(options));
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
