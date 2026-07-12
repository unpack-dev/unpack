import assert from "node:assert/strict";

export default { plugins: [(compiler) => compiler.hooks.compilation.tap("plugin", (compilation) => {
  compilation.hooks.processAssets.tap("plugin", () => {
    assert.equal(compilation.entrypoints.get("main").getRuntimeChunk().name, "main");
  });
})] };
