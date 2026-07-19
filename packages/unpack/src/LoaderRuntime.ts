// JavaScript loader execution used by the native NormalModuleFactory bridge.

import { dirname } from "node:path";

import { LoaderFunction, LoaderState, require } from "./binding.js";
import type { LoaderRunResult, LoaderModule, NativeLoaderContext } from "./binding.js";

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
    serializedOptions: string,
    nativeContext: NativeLoaderContext
  ): Promise<LoaderRunResult> => {
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

    return new Promise<LoaderRunResult>((resolve, reject) => {
      const requests = new Map<string, { request: string; kind: "load" | "import" }>();
      const fileDependencies = new Set<string>();
      let callbackRequested = false;
      let settled = false;
      const complete = (error: unknown, transformedSource?: unknown): void => {
        if (settled) return;
        settled = true;
        if (error != null) {
          reject(error);
        } else if (typeof transformedSource === "string") {
          resolve({
            source: transformedSource,
            requests: [...requests.values()],
            fileDependencies: [...fileDependencies]
          });
        } else {
          reject(new TypeError(`loader ${loaderPath} callback must provide a string`));
        }
      };
      const callback = (error: unknown, transformedSource?: unknown): void => {
        complete(error, transformedSource);
      };

      const loadModule = (
        request: string,
        loadCallback: (
          error: Error | null,
          source?: string,
          sourceMap?: null,
          module?: LoaderModule
        ) => void
      ): void => {
        if (typeof request !== "string" || typeof loadCallback !== "function") {
          throw new TypeError("this.loadModule requires a request and callback");
        }
        void nativeContext.loadModule(request, "load").then(
          ({ resource, source, identifier, fileDependencies: loadedDependencies }) => {
            requests.set(`load\0${request}`, { request, kind: "load" });
            for (const dependency of loadedDependencies) fileDependencies.add(dependency);
            loadCallback(null, source, null, {
              resource,
              identifier: () => identifier
            });
          },
          (error) => loadCallback(toError(error))
        );
      };

      const importModule = (
        request: string,
        importOptions: Record<string, unknown> | ((error: Error | null, exports?: unknown) => void) = {},
        importCallback?: (error: Error | null, exports?: unknown) => void
      ): Promise<unknown> | void => {
        const callback = typeof importOptions === "function" ? importOptions : importCallback;
        const options = typeof importOptions === "function" ? {} : importOptions;
        if (typeof request !== "string") throw new TypeError("this.importModule requires a request");
        if (options == null || typeof options !== "object" || Array.isArray(options)) {
          throw new TypeError("this.importModule options must be an object");
        }
        const unsupported = Object.keys(options).filter(
          (key) => !["layer", "publicPath", "baseUri"].includes(key)
        );
        if (unsupported.length > 0) {
          throw new TypeError(`this.importModule does not support option '${unsupported[0]}'`);
        }
        if (Object.keys(options).length > 0) {
          throw new TypeError("this.importModule options are not supported yet");
        }
        const imported = this.#importModule(request, nativeContext).then(({ exports, dependencies }) => {
          requests.set(`import\0${request}`, { request, kind: "import" });
          for (const dependency of dependencies) fileDependencies.add(dependency);
          return exports;
        });
        if (!callback) return imported;
        void imported.then(
          (exports) => callback(null, exports),
          (error) => callback(toError(error))
        );
      };

      let result: unknown;
      try {
        result = state.loader.call(
          {
            resourcePath,
            rootContext: this.rootContext,
            sourceMap: false,
            getOptions: () => JSON.parse(serializedOptions) as Record<string, unknown>,
            loadModule,
            importModule,
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

  async #importModule(
    request: string,
    nativeContext: NativeLoaderContext
  ): Promise<{ exports: unknown; dependencies: string[] }> {
    const loaded = await nativeContext.loadModule(request, "import");
    const executed = await createModuleUrl(loaded, nativeContext, new Set());
    return { exports: await import(executed.url), dependencies: executed.dependencies };
  }
}

async function createModuleUrl(
  loaded: {
    source: string;
    resource: string;
    identifier: string;
    fileDependencies: string[];
    dependencyRequests: string[];
  },
  nativeContext: NativeLoaderContext,
  ancestors: Set<string>
): Promise<{ url: string; dependencies: string[] }> {
  if (ancestors.has(loaded.identifier)) {
    throw new Error(`circular importModule execution is not supported yet: ${loaded.identifier}`);
  }
  const nextAncestors = new Set(ancestors).add(loaded.identifier);
  const dependencyRequests = new Set(loaded.dependencyRequests);
  // Rust's parser owns dependency discovery. This expression only locates the already-known
  // string specifiers so the host ESM evaluator can link them to their built module URLs.
  const matches = [...loaded.source.matchAll(/\b(?:from\s*|import\s*(?:\(\s*)?)(["'])([^"']+)\1/g)]
    .filter((match) => dependencyRequests.has(match[2]));
  const replacements = await Promise.all(matches.map(async (match) => {
    const request = match[2];
    const requestOffset = match.index + match[0].lastIndexOf(request);
    const dependency = await nativeContext.loadModule(request, "import", dirname(loaded.resource));
    return {
      start: requestOffset,
      end: requestOffset + request.length,
      executed: await createModuleUrl(dependency, nativeContext, nextAncestors)
    };
  }));
  let source = loaded.source;
  for (const replacement of replacements.reverse()) {
    source = `${source.slice(0, replacement.start)}${replacement.executed.url}${source.slice(replacement.end)}`;
  }
  return {
    url: `data:text/javascript;base64,${Buffer.from(source).toString("base64")}#${encodeURIComponent(loaded.identifier)}`,
    dependencies: [
      ...loaded.fileDependencies,
      ...replacements.flatMap((replacement) => replacement.executed.dependencies)
    ]
  };
}

function toError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}
