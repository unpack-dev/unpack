import { createRequire } from "node:module";
import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

import unpack from "@unpack-js/core";
import type { Stats } from "@unpack-js/core";

const require = createRequire(import.meta.url);

test("asset/resource emits the original bytes and exports its filename", async () => {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-asset-resource-"));
  const outputPath = join(fixture, "dist");
  const source = Buffer.from([0, 1, 2, 250, 255]);

  try {
    await writeFile(
      join(fixture, "index.js"),
      'import filename from "./data.bin"; export { filename };'
    );
    await writeFile(join(fixture, "data.bin"), source);

    const { err, stats } = await runCompiler({
      context: fixture,
      entry: "./index.js",
      output: { path: outputPath },
      sourcemap: false,
      module: {
        rules: [{ test: /[.]bin$/, type: "asset/resource" }]
      }
    });

    assert.equal(err, null);
    assert.equal(stats?.hasErrors(), false);
    const files = await readdir(outputPath);
    const resource = files.find((file) => file !== "main.js");
    assert.match(resource ?? "", /^[a-f0-9]+[.]bin$/);
    assert.deepEqual(await readFile(join(outputPath, resource!)), source);
    assert.equal(require(join(outputPath, "main.js")).filename, resource);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("asset/inline exports a base64 data URI without emitting another asset", async () => {
  const result = await compileAsset("asset/inline", Buffer.from([0, 1, 2, 250, 255]));
  try {
    assert.deepEqual(result.files, ["main.js"]);
    assert.equal(
      result.exported,
      "data:application/octet-stream;base64,AAEC+v8="
    );
  } finally {
    await result.cleanup();
  }
});

test("asset/inline uses the resource MIME type", async () => {
  const result = await compileAsset("asset/inline", Buffer.from("x"), "avif");
  try {
    assert.equal(result.exported, "data:image/avif;base64,eA==");
  } finally {
    await result.cleanup();
  }
});

test("asset/source exports the resource text", async () => {
  const source = Buffer.from([0, 1, 2, 250, 255]);
  const result = await compileAsset("asset/source", source);
  try {
    assert.deepEqual(result.files, ["main.js"]);
    assert.equal(result.exported, source.toString("utf8"));
  } finally {
    await result.cleanup();
  }
});

test("asset uses webpack's default 8096-byte inline threshold", async () => {
  const inline = await compileAsset("asset", Buffer.alloc(8096, 1));
  try {
    assert.deepEqual(inline.files, ["main.js"]);
    assert.match(inline.exported, /^data:application\/octet-stream;base64,/);
  } finally {
    await inline.cleanup();
  }

  const resource = await compileAsset("asset", Buffer.alloc(8097, 1));
  try {
    assert.equal(resource.files.length, 2);
    assert.match(resource.exported, /^[a-f0-9]+[.]bin$/);
    assert.equal((await readFile(join(resource.outputPath, resource.exported))).length, 8097);
  } finally {
    await resource.cleanup();
  }
});

async function compileAsset(
  type: "asset" | "asset/inline" | "asset/source",
  source: Buffer,
  extension = "bin"
) {
  const fixture = await mkdtemp(join(tmpdir(), "unpack-asset-module-"));
  const outputPath = join(fixture, "dist");
  await writeFile(
    join(fixture, "index.js"),
    `import value from "./data.${extension}"; export { value };`
  );
  await writeFile(join(fixture, `data.${extension}`), source);
  const { err, stats } = await runCompiler({
    context: fixture,
    entry: "./index.js",
    output: { path: outputPath },
    sourcemap: false,
    module: { rules: [{ test: /data[.]/, type }] }
  });
  assert.equal(err, null);
  assert.equal(stats?.hasErrors(), false);
  return {
    outputPath,
    files: (await readdir(outputPath)).sort(),
    exported: require(join(outputPath, "main.js")).value as string,
    cleanup: () => rm(fixture, { recursive: true, force: true })
  };
}

function runCompiler(options: Parameters<typeof unpack>[0]) {
  return new Promise<{ err: Error | null; stats?: Stats }>((resolve) => {
    unpack(options, (err, stats) => resolve({ err, stats }));
  });
}
