import { RawSource } from "webpack-sources";

export default { plugins: [{ apply(compiler) { compiler.hooks.compilation.tap("plugin", (compilation) => {
  compilation.hooks.processAssets.tapPromise("plugin", async () => {
    const files = [...compilation.entrypoints].flatMap(([name, entrypoint]) => entrypoint.getFiles().map((file) => `${name}:${file}`));
    compilation.emitAsset("inspect.txt", new RawSource(files.join("\n")));
  });
}); } }] };
