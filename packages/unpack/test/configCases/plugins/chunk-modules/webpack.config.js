import assert from "node:assert/strict";
import webpackSources from "webpack-sources";

const { RawSource } = webpackSources;

export default { plugins: [(compiler) => compiler.hooks.compilation.tap("plugin", (compilation) => {
  compilation.hooks.processAssets.tap("plugin", () => {
    const chunks = [...compilation.chunks];
    assert.equal(chunks.length > 0, true);
    assert.equal([...compilation.chunkGraph.getChunkModulesIterable(chunks[0])].length > 0, true);
    assert.equal([...compilation.chunkGraph.getChunkEntryModulesIterable(chunks[0])].length > 0, true);
    const contexts = [...compilation.chunkGraph.getChunkModulesIterable(chunks[0])].map((module) => module.context);
    assert.equal(contexts.every((context) => typeof context === "string"), true);
    compilation.emitAsset("chunk-modules.txt", new RawSource(contexts.join("\n")));
  });
})] };
