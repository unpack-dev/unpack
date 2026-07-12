// Internal N-API transport shared by the webpack-shaped JavaScript facades.

import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { InfrastructureLogEvent, NormalizedOptions } from "./config/normalization.js";
import { StatsAsset, StatsError, WatchDependencySets } from "./Stats.js";

export interface NativeStatsJson {
  errors: StatsError[];
  warnings?: StatsError[];
  assets: StatsAsset[];
  outputPath?: string;
  output_path?: string;
  watchDependencies?: WatchDependencySets;
  watch_dependencies?: WatchDependencySets;
}

export interface NativeCompilation {
  modules(): NativeModule[];
  takeAssetSources(): NativeAssetSource[];
  clearAssetSources(): void;
  outgoingConnections(moduleHandle: number): NativeModuleGraphConnection[];
  incomingConnections(moduleHandle: number): NativeModuleGraphConnection[];
  connectionsByHandle(connectionHandles: number[]): NativeModuleGraphConnection[];
  chunks(): NativeChunk[];
  chunkGroups(): NativeChunkGroup[];
  chunkEntryModules(chunkHandle: number): number[];
  chunkModules(chunkHandle: number): number[];
  moduleChunks(moduleHandle: number): number[];
  moduleId(moduleHandle: number): string | number | null;
  returnModuleGraphLease(): void;
}

export interface NativeChunkGroup {
  handle: number;
  name?: string | null;
  chunkHandles: number[];
  runtimeChunkHandle?: number | null;
  files: string[];
  isEntrypoint: boolean;
}

export interface NativeAssetSource {
  name: string;
  source: Uint8Array;
}

export interface NativeAssets {
  compilation(): NativeCompilation;
  takeAssetSources(): NativeAssetSource[];
  replaceAssetSources(assets: NativeAssetSource[]): void;
  returnAssetsLease(): void;
}

export interface NativeModule {
  handle: number;
  identifier: string;
  resource: string;
  type: string;
  providedExports?: string[] | null;
  usedExports?: string[] | null;
  allExportsUsed?: boolean;
}

export interface NativeModuleGraphConnection {
  handle: number;
  originModuleHandle?: number | null;
  moduleHandle: number;
  resolvedModuleHandle: number;
  dependencyType?: string;
  dependency_type?: string;
  request?: string | null;
  weak: boolean;
  parentBlockIndex?: number;
  parent_block_index?: number;
}

export interface NativeChunk {
  handle: number;
  name?: string | null;
  renderId?: string | number | null;
  render_id?: string | number | null;
}

export interface NativeRunResult {
  error?: {
    name: string;
    message: string;
  } | null;
  stats?: NativeStatsJson | null;
  compilation?: NativeCompilation | null;
  logs?: InfrastructureLogEvent[] | null;
}

export interface NativeFlushResult {
  error?: {
    name: string;
    message: string;
  } | null;
  logs?: InfrastructureLogEvent[] | null;
}

export interface NativeCompiler {
  run(): Promise<NativeRunResult>;
  settleCache(): Promise<NativeFlushResult>;
  shutdown(): Promise<NativeFlushResult>;
  close(): void;
}

export interface NativeBinding {
  createCompiler(
    options: NormalizedOptions,
    loaderRunner?: (
      loader: string,
      resource: string,
      source: string,
      options: string
    ) => Promise<string>,
    compilation?: (compilation: NativeCompilation) => Promise<void>,
    finishModules?: (compilation: NativeCompilation) => Promise<void>,
    processAssets?: (assets: NativeAssets) => Promise<void>
  ): NativeCompiler;
}

export const require = createRequire(import.meta.url);

export const native = require("./unpack_node.node") as NativeBinding;

export const { RawSource } = require("webpack-sources") as typeof import("webpack-sources");

export const unpackJavaScriptPath = fileURLToPath(new URL("./webpack.js", import.meta.url));

// The native addon is the compiled closure of the Rust compiler, parser, and
// resolver; together with the JS entry and package metadata it is the runtime toolchain.
export const unpackToolchainBuildDependencies = [
  ...[
    "binding.js",
    "Chunk.js",
    "ChunkGraph.js",
    "ChunkGroup.js",
    "Compilation.js",
    "Compiler.js",
    "config/normalization.js",
    "Dependency.js",
    "Entrypoint.js",
    "ExportsInfo.js",
    "index.js",
    "LoaderRuntime.js",
    "Module.js",
    "ModuleGraph.js",
    "ModuleGraphConnection.js",
    "Stats.js",
    "util.js",
    "Watching.js",
    "webpack.js"
  ].map((path) => fileURLToPath(new URL(path, import.meta.url))),
  require.resolve("./unpack_node.node"),
  resolve(dirname(unpackJavaScriptPath), "../package.json")
];

export type LoaderFunction = (
  this: {
    resourcePath: string;
    rootContext: string;
    sourceMap: false;
    getOptions(): Record<string, unknown>;
    async(): (error: unknown, source?: unknown) => void;
  },
  source: string
) => unknown;

export type LoaderState =
  | { failed: false; loader: LoaderFunction }
  | { failed: true; error: unknown };
