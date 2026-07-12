import assert from "node:assert/strict";

export default { plugins: [{ apply(compiler) {
  const inputFileSystem = Object.create(compiler.inputFileSystem);
  compiler.inputFileSystem = inputFileSystem;
  compiler.hooks.beforeCompile.tap("plugin", () => {
    assert.equal(compiler.inputFileSystem, inputFileSystem);
  });
} }] };
