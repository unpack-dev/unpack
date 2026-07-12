// Organized to match webpack's lib/webpack.js responsibility.

import { Compiler, CompilerImpl, RunCallback } from "./Compiler.js";
import { UnpackOptions, normalizeOptions } from "./config/normalization.js";
import { assertFunction, defer, toError } from "./util.js";

export interface WebpackPluginInstance {
  apply(compiler: Compiler): void;
}

export type WebpackPluginFunction = (
  this: Compiler,
  compiler: Compiler
) => void;

export type WebpackPlugin =
  | WebpackPluginInstance
  | WebpackPluginFunction
  | false
  | null
  | undefined
  | 0
  | "";

export default function unpack(
  options: UnpackOptions,
  callback: RunCallback
): Compiler | null;

export default function unpack(
  options: UnpackOptions,
  callback?: undefined
): Compiler;

export default function unpack(
  options: UnpackOptions,
  callback?: RunCallback
): Compiler | null {
  if (callback !== undefined) {
    assertFunction(callback, "callback");
  }

  let compiler: Compiler;
  try {
    const normalizedOptions = normalizeOptions(options);
    const plugins = normalizePlugins(options.plugins);
    compiler = new CompilerImpl(normalizedOptions);
    applyPlugins(plugins, compiler);
  } catch (error) {
    if (callback === undefined) {
      throw error;
    }

    const constructionError = toError(error, "InfrastructureError");
    defer(() => callback(constructionError));
    return null;
  }
  if (callback) {
    compiler.run(callback);
  }
  return compiler;
}

export function normalizePlugins(plugins: unknown): Array<WebpackPluginInstance | WebpackPluginFunction> {
  if (plugins === undefined) {
    return [];
  }
  if (!Array.isArray(plugins)) {
    throw new TypeError("options.plugins must be an array");
  }

  const normalized: Array<WebpackPluginInstance | WebpackPluginFunction> = [];
  for (const [index, plugin] of plugins.entries()) {
    if (
      plugin === false ||
      plugin === null ||
      plugin === undefined ||
      plugin === 0 ||
      plugin === ""
    ) {
      continue;
    }
    if (typeof plugin === "function") {
      normalized.push(plugin as WebpackPluginFunction);
      continue;
    }
    if (
      typeof plugin === "object" &&
      typeof (plugin as { apply?: unknown }).apply === "function"
    ) {
      normalized.push(plugin as WebpackPluginInstance);
      continue;
    }
    throw new TypeError(
      `options.plugins[${index}] must be a function, a plugin with an apply method, or falsy`
    );
  }
  return normalized;
}

export function applyPlugins(
  plugins: readonly (WebpackPluginInstance | WebpackPluginFunction)[],
  compiler: Compiler
): void {
  for (const plugin of plugins) {
    if (typeof plugin === "function") {
      (plugin as WebpackPluginFunction).call(compiler, compiler);
    } else {
      plugin.apply(compiler);
    }
  }
}
