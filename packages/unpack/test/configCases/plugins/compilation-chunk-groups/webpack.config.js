import assert from "node:assert/strict";

export default { entry: { a: "./a.js", b: "./b.js" }, plugins: [(compiler) => compiler.hooks.compilation.tap("plugin", (compilation) => {
  compilation.hooks.processAssets.tap("plugin", () => {
    assert.equal(compilation.chunkGroups.length >= 2, true);
    assert.deepEqual([...compilation.namedChunkGroups.keys()].sort(), ["a", "b"]);
    assert.deepEqual(compilation.namedChunkGroups.get("a").getFiles(), ["a.js"]);
  });
})] };
