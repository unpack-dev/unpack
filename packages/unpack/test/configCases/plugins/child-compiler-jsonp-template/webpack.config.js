import assert from "node:assert/strict";

export default { plugins: [{ apply(compiler) {
  compiler.hooks.make.tapAsync("plugin", (compilation, done) => {
    const child = compilation.createChildCompiler("child", {}, [
      new compiler.webpack.EntryPlugin(compiler.context, "./child.js", {
        name: "child"
      })
    ]);
    child.runAsChild((error, entries) => {
      assert.equal(error, null);
      assert.equal(entries.length, 1);
      done();
    });
  });
} }] };
