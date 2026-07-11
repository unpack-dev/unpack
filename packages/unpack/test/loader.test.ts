import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

import unpack from "@unpack-js/core";
import type { Compiler, Stats, UnpackOptions } from "@unpack-js/core";

test("a synchronous CommonJS loader transforms a module before parsing", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-loader-"));
  const entry = join(fixture, "src/index.js");
  const resource = join(fixture, "src/value.benchdata");
  const loader = join(fixture, "loaders/benchmark-loader.cjs");

  await writeFixtureFile(entry, 'import value from "./value.benchdata?query#fragment";\nexport { value };\n');
  await writeFixtureFile(resource, "42\n");
  await writeFixtureFile(
    loader,
    [
      "module.exports = function benchmarkLoader(source) {",
      `  if (!this.resourcePath.endsWith(${JSON.stringify(join("src", "value.benchdata"))})) throw new Error("unexpected resourcePath");`,
      `  if (this.rootContext !== ${JSON.stringify(fixture)}) throw new Error("unexpected rootContext");`,
      "  const value = Number.parseInt(source.trim(), 10);",
      '  return `export default ${value};`;',
      "};",
      ""
    ].join("\n")
  );

  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    output: { path: join(fixture, "dist") },
    sourcemap: false,
    module: {
      rules: [{ test: /\.benchdata$/, loader }]
    }
  });

  try {
    const stats = await runCompiler(compiler);
    assert.equal(stats.hasErrors(), false);
    assert.equal(stats.toJson().watchDependencies.files.includes(loader), true);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /42/);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("an asynchronous loader callback transforms a module with rule options", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-async-loader-"));
  const loader = join(fixture, "loader.cjs");
  await writeFixtureFile(
    join(fixture, "src/index.js"),
    'import value from "./value.benchdata";\nexport { value };\n'
  );
  await writeFixtureFile(join(fixture, "src/value.benchdata"), "6\n");
  await writeFixtureFile(
    loader,
    [
      "module.exports = function (source) {",
      "  const callback = this.async();",
      "  const { multiplier } = this.getOptions();",
      "  setTimeout(() => {",
      "    callback(null, `export default ${Number.parseInt(source.trim(), 10) * multiplier};`);",
      "  }, 5);",
      "};",
      ""
    ].join("\n")
  );
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    output: { path: join(fixture, "dist") },
    sourcemap: false,
    module: {
      rules: [{ test: /\.benchdata$/, loader, options: { multiplier: 7 } }]
    }
  });

  try {
    const stats = await runCompiler(compiler);
    assert.equal(stats.hasErrors(), false);
    assert.match(await readFile(join(fixture, "dist/main.js"), "utf8"), /42/);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("a loader is not loaded when its rule does not match", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-loader-no-match-"));
  await writeFixtureFile(join(fixture, "src/index.js"), 'import value from "./value.js";\nexport { value };\n');
  await writeFixtureFile(join(fixture, "src/value.js"), "export default 7;\n");
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    output: { path: join(fixture, "dist") },
    sourcemap: false,
    module: {
      rules: [{ test: /\.benchdata$/, loader: join(fixture, "missing-loader.cjs") }]
    }
  });

  try {
    const stats = await runCompiler(compiler);
    assert.equal(stats.hasErrors(), false);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("loader failures and invalid return values are Compilation errors", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-loader-errors-"));
  const loader = join(fixture, "loader.cjs");
  await writeFixtureFile(join(fixture, "src/index.js"), 'import value from "./value.benchdata";\nexport { value };\n');
  await writeFixtureFile(join(fixture, "src/value.benchdata"), "1\n");
  await writeFixtureFile(loader, 'module.exports = function () { throw new Error("loader exploded"); };\n');
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    output: { path: join(fixture, "dist") },
    sourcemap: false,
    module: { rules: [{ test: /\.benchdata$/, loader }] }
  });

  try {
    const thrown = await runCompiler(compiler);
    assert.equal(thrown.hasErrors(), true);
    assert.match(thrown.toJson().errors[0]?.message ?? "", /loader exploded/);

    await writeFixtureFile(loader, "module.exports = function () { return 42; };\n");
    const invalidReturn = await runCompiler(compiler);
    assert.equal(invalidReturn.hasErrors(), true);
    assert.match(invalidReturn.toJson().errors[0]?.message ?? "", /must return a string/);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("module rules reject unsupported loader configuration synchronously", () => {
  const absoluteLoader = join(tmpdir(), "loader.cjs");
  const base = { entry: "./src/index.js", sourcemap: false } satisfies UnpackOptions;
  const invalidOptions: Array<[UnpackOptions, RegExp]> = [
    [{ ...base, module: { rules: [{ test: /x/i, loader: absoluteLoader }] } }, /must not use flags/],
    [{ ...base, module: { rules: [{ test: /x/, loader: "./loader.cjs" }] } }, /absolute path/],
    [{ entry: "./src/index.js", module: { rules: [{ test: /x/, loader: absoluteLoader }] } }, /sourcemap must be false/],
    [{ ...base, module: { rules: [{ test: /x(?=y)/, loader: absoluteLoader }] } }, /look-around/],
    [
      {
        ...base,
        module: {
          rules: [{ test: /x/, loader: absoluteLoader, use: [] }]
        }
      } as unknown as UnpackOptions,
      /unknown option 'use'/
    ]
  ];

  for (const [options, expected] of invalidOptions) {
    assert.throws(() => unpack(options), expected);
  }
});

test("memory cache invalidates loader output when the resource or loader changes", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-loader-cache-"));
  const resource = join(fixture, "src/value.benchdata");
  const loader = join(fixture, "loader.cjs");
  const output = join(fixture, "dist/main.js");
  await writeFixtureFile(join(fixture, "src/index.js"), 'import value from "./value.benchdata";\nexport { value };\n');
  await writeFixtureFile(resource, "2\n");
  await writeFixtureFile(loader, multiplierLoader(2));
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    output: { path: dirname(output) },
    sourcemap: false,
    cache: true,
    snapshot: { module: { timestamp: false, hash: true } },
    module: { rules: [{ test: /\.benchdata$/, loader }] }
  });

  try {
    assert.equal((await runCompiler(compiler)).hasErrors(), false);
    assert.match(await readFile(output, "utf8"), /4/);

    await writeFixtureFile(resource, "3\n");
    assert.equal((await runCompiler(compiler)).hasErrors(), false);
    assert.match(await readFile(output, "utf8"), /6/);

    await writeFixtureFile(loader, multiplierLoader(3));
    assert.equal((await runCompiler(compiler)).hasErrors(), false);
    assert.match(await readFile(output, "utf8"), /9/);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("one loader module is reused per Compilation and cache hits skip it", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-loader-reuse-"));
  const first = join(fixture, "src/first.benchdata");
  const second = join(fixture, "src/second.benchdata");
  const loader = join(fixture, "loader.cjs");
  const counter = join(fixture, "loader-calls.txt");
  await writeFixtureFile(
    join(fixture, "src/index.js"),
    'import first from "./first.benchdata";\nimport second from "./second.benchdata";\nexport { first, second };\n'
  );
  await writeFixtureFile(first, "1\n");
  await writeFixtureFile(second, "2\n");
  await writeFixtureFile(
    loader,
    [
      'const { appendFileSync } = require("node:fs");',
      `appendFileSync(${JSON.stringify(counter)}, "load\\n");`,
      "module.exports = function (source) {",
      `  appendFileSync(${JSON.stringify(counter)}, "call\\n");`,
      "  return `export default ${Number.parseInt(source.trim(), 10)};`;",
      "};",
      ""
    ].join("\n")
  );
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    sourcemap: false,
    cache: true,
    snapshot: { module: { timestamp: false, hash: true } },
    module: { rules: [{ test: /\.benchdata$/, loader }] }
  });

  try {
    assert.equal((await runCompiler(compiler)).hasErrors(), false);
    assert.deepEqual((await readFile(counter, "utf8")).trim().split("\n"), ["load", "call", "call"]);

    assert.equal((await runCompiler(compiler)).hasErrors(), false);
    assert.deepEqual((await readFile(counter, "utf8")).trim().split("\n"), ["load", "call", "call"]);

    await writeFixtureFile(first, "3\n");
    assert.equal((await runCompiler(compiler)).hasErrors(), false);
    assert.deepEqual((await readFile(counter, "utf8")).trim().split("\n"), [
      "load",
      "call",
      "call",
      "load",
      "call"
    ]);
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

test("a loader load failure is reused within one Compilation", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-loader-failed-load-"));
  const loader = join(fixture, "loader.cjs");
  const counter = join(fixture, "loader-loads.txt");
  await writeFixtureFile(
    join(fixture, "src/index.js"),
    'import "./first.benchdata";\nimport "./second.benchdata";\n'
  );
  await writeFixtureFile(join(fixture, "src/first.benchdata"), "1\n");
  await writeFixtureFile(join(fixture, "src/second.benchdata"), "2\n");
  await writeFixtureFile(
    loader,
    [
      'const { appendFileSync } = require("node:fs");',
      `appendFileSync(${JSON.stringify(counter)}, "load\\n");`,
      'throw new Error("loader failed to load");',
      ""
    ].join("\n")
  );
  const compiler = unpack({
    context: fixture,
    entry: "./src/index.js",
    sourcemap: false,
    module: { rules: [{ test: /\.benchdata$/, loader }] }
  });

  try {
    const stats = await runCompiler(compiler);
    assert.equal(stats.hasErrors(), true);
    assert.equal((await readFile(counter, "utf8")).trim(), "load");
  } finally {
    await closeCompiler(compiler);
    await rm(fixture, { recursive: true, force: true });
  }
});

function multiplierLoader(multiplier: number): string {
  return [
    "module.exports = function (source) {",
    `  return \`export default \${Number.parseInt(source.trim(), 10) * ${multiplier}};\`;`,
    "};",
    ""
  ].join("\n");
}

async function writeFixtureFile(path: string, contents: string): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, contents);
}

function runCompiler(compiler: Compiler): Promise<Stats> {
  return new Promise((resolve, reject) => {
    compiler.run((error, stats) => {
      if (error) {
        reject(error);
      } else if (!stats) {
        reject(new Error("compiler completed without Stats"));
      } else {
        resolve(stats);
      }
    });
  });
}

function closeCompiler(compiler: Compiler): Promise<void> {
  return new Promise((resolve, reject) => {
    compiler.close((error) => (error ? reject(error) : resolve()));
  });
}
