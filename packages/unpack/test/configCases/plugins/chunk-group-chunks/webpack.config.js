import assert from "node:assert/strict";

export default {
  entry: { a: "./entry1.js", b: "./entry2.js" },
  plugins: [(compiler) => compiler.hooks.compilation.tap("plugin", (compilation) => {
    compilation.hooks.processAssets.tap("plugin", () => {
      assert.deepEqual(compilation.entrypoints.get("a").chunks.map((chunk) => chunk.name), ["a"]);
      assert.deepEqual(compilation.entrypoints.get("b").chunks.map((chunk) => chunk.name), ["b"]);
    });
  })]
};
