import assert from "node:assert/strict";
import webpackSources from "webpack-sources";

const { RawSource } = webpackSources;

export default { plugins: [{ apply(compiler) { compiler.hooks.compilation.tap("plugin", (compilation) => {
  compilation.hooks.processAssets.tapPromise("plugin", async (assets) => {
    assets["dup.txt"] = new RawSource("dup");
    assert.equal("dup.txt" in assets, true);
    delete assets["dup.txt"];
    assert.equal("dup.txt" in assets, false);
  });
}); } }] };
