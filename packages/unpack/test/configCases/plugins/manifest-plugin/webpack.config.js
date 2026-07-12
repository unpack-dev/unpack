import assert from "node:assert/strict";
import webpackSources from "webpack-sources";

const { RawSource } = webpackSources;

export default { plugins: [{ apply(compiler) {
  compiler.hooks.thisCompilation.tap("plugin", (compilation) => {
    compilation.hooks.processAssets.tap("plugin", () => {
      compilation.emitAsset("third.party.js", new RawSource("third party"));
      assert.equal(compilation.getAsset("third.party.js") !== undefined, true);
    });
  });
} }] };
