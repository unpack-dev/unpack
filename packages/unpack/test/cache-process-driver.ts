import { randomUUID } from "node:crypto";

import unpack from "@unpack-js/core";
import webpack from "webpack";

import type {
  CacheProcessObservation,
  CacheProcessRequest
} from "./cache-process-harness.js";

const request = JSON.parse(process.argv[2] ?? "null") as CacheProcessRequest;

void observeBuild(request).then((observation) => {
  process.stdout.write(`${JSON.stringify(observation)}\n`);
});

async function observeBuild(
  buildRequest: CacheProcessRequest
): Promise<CacheProcessObservation> {
  const base = {
    pid: process.pid,
    instanceId: randomUUID(),
    synchronousError: false,
    error: null,
    hasStats: false,
    hasErrors: null,
    assets: [],
    outputPath: null,
    cacheWork: null
  } satisfies CacheProcessObservation;

  let compiler: ReturnType<typeof unpack> | ReturnType<typeof webpack>;
  try {
    const { outputPath, ...options } = buildRequest.options;
    compiler =
      buildRequest.bundler === "webpack"
        ? webpack({
            ...options,
            entry: options.entry ?? "./src/index.js",
            devtool: false,
            infrastructureLogging: { level: "none" },
            output: { path: outputPath, filename: "main.js" }
          } as unknown as Parameters<typeof webpack>[0])
        : unpack({
            ...options,
            entry: options.entry ?? "./src/index.js",
            sourcemap: false,
            infrastructureLogging: { level: "none" },
            output: { path: outputPath }
          } as Parameters<typeof unpack>[0]);
  } catch (error) {
    return {
      ...base,
      synchronousError: true,
      error: errorDescription(error)
    };
  }

  const run = await runCompiler(compiler, buildRequest.bundler);
  const closeError = await closeCompiler(compiler);
  return {
    ...base,
    error: run.error ?? closeError,
    hasStats: run.hasStats,
    hasErrors: run.hasErrors,
    assets: run.assets,
    outputPath: run.outputPath
  };
}

async function runCompiler(
  compiler: ReturnType<typeof unpack> | ReturnType<typeof webpack>,
  bundler: CacheProcessRequest["bundler"]
) {
  return new Promise<{
    error: { name: string; message: string } | null;
    hasStats: boolean;
    hasErrors: boolean | null;
    assets: string[];
    outputPath: string | null;
  }>((resolve) => {
    compiler.run((error, stats) => {
      if (error || !stats) {
        resolve({
          error: error ? errorDescription(error) : null,
          hasStats: stats !== undefined,
          hasErrors: stats?.hasErrors() ?? null,
          assets: [],
          outputPath: null
        });
        return;
      }

      const json =
        bundler === "webpack"
          ? (stats as unknown as import("webpack").Stats).toJson({
              assets: true,
              outputPath: true
            })
          : (stats as import("@unpack-js/core").Stats).toJson();
      resolve({
        error: null,
        hasStats: true,
        hasErrors: stats.hasErrors(),
        assets: (json.assets ?? []).flatMap((asset) =>
          asset.name === undefined ? [] : [asset.name]
        ).sort(),
        outputPath: json.outputPath ?? null
      });
    });
  });
}

async function closeCompiler(
  compiler: ReturnType<typeof unpack> | ReturnType<typeof webpack>
) {
  return new Promise<{ name: string; message: string } | null>((resolve) => {
    compiler.close((error) => resolve(error ? errorDescription(error) : null));
  });
}

function errorDescription(error: unknown) {
  return error instanceof Error
    ? { name: error.name, message: error.message }
    : { name: "Error", message: String(error) };
}
