import assert from "node:assert/strict";

export default { plugins: [(compiler) => compiler.hooks.compilation.tap("plugin", (compilation) => {
  compilation.hooks.processAssets.tap("plugin", () => {
    const chunks = [...compilation.chunks];
    assert.equal(chunks.length > 0, true);
    assert.equal([...compilation.chunkGraph.getChunkModulesIterable(chunks[0])].length > 0, true);
    assert.equal([...compilation.chunkGraph.getChunkEntryModulesIterable(chunks[0])].length > 0, true);
  });
})] };
