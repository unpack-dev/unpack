// Ported from Rspack's plugins/internal-processAssets config case.
import assert from "node:assert/strict";
import webpackSources from "webpack-sources";

const { ConcatSource, RawSource } = webpackSources;

export default {
  plugins: [
    {
      name: "test",
      apply(compiler) {
        compiler.hooks.compilation.tap("compilation", (compilation) => {
          compilation.hooks.processAssets.tapPromise("Test1", async (assets) => {
            for (const [name, source] of Object.entries(assets)) {
              compilation.updateAsset(
                name,
                new ConcatSource(new RawSource("//banner;\n"), source)
              );
            }
          });

          compilation.hooks.processAssets.tapPromise("Test2", async (assets) => {
            assert.equal(Object.keys(assets).length, 1);
            assert.equal(Object.getOwnPropertyNames(assets).length, 1);
            assert.equal(Reflect.ownKeys(assets).length, 1);
            assert.equal("main.js" in assets, true);
            assert.equal(assets["main.js"].source().startsWith("//banner;\n"), true);
          });
        });
      }
    }
  ]
};
