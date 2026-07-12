// JavaScript loader execution used by the native NormalModuleFactory bridge.

import { LoaderFunction, LoaderState, require } from "./binding.js";

export class LoaderRuntime {
  readonly #loaders = new Map<string, LoaderState>();

  constructor(private readonly rootContext: string) {}

  beginCompilation(): void {
    this.#loaders.clear();
  }

  readonly run = async (
    loaderPath: string,
    resourcePath: string,
    source: string,
    serializedOptions: string
  ): Promise<string> => {
    let state = this.#loaders.get(loaderPath);
    if (state === undefined) {
      try {
        const resolvedLoaderPath = require.resolve(loaderPath);
        delete require.cache[resolvedLoaderPath];
        const loaded: unknown = require(resolvedLoaderPath);
        if (typeof loaded !== "function") {
          throw new TypeError(`loader ${loaderPath} must export a CommonJS function`);
        }
        state = { failed: false, loader: loaded as LoaderFunction };
      } catch (error) {
        state = { failed: true, error };
      }
      this.#loaders.set(loaderPath, state);
    }
    if (state.failed) throw state.error;

    return new Promise<string>((resolve, reject) => {
      let callbackRequested = false;
      let settled = false;
      const complete = (error: unknown, transformedSource?: unknown): void => {
        if (settled) return;
        settled = true;
        if (error != null) {
          reject(error);
        } else if (typeof transformedSource === "string") {
          resolve(transformedSource);
        } else {
          reject(new TypeError(`loader ${loaderPath} callback must provide a string`));
        }
      };
      const callback = (error: unknown, transformedSource?: unknown): void => {
        complete(error, transformedSource);
      };

      let result: unknown;
      try {
        result = state.loader.call(
          {
            resourcePath,
            rootContext: this.rootContext,
            sourceMap: false,
            getOptions: () => JSON.parse(serializedOptions) as Record<string, unknown>,
            async: () => {
              callbackRequested = true;
              return callback;
            }
          },
          source
        );
      } catch (error) {
        complete(error);
        return;
      }

      if (typeof result === "string") {
        complete(null, result);
      } else if (result instanceof Promise) {
        result.then(
          (transformedSource) => complete(null, transformedSource),
          (error) => complete(error)
        );
      } else if (!callbackRequested) {
        complete(
          new TypeError(
            `loader ${loaderPath} must return a string, a Promise, or request a callback`
          )
        );
      }
    });
  };
}
