// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/index.js

export { default } from "./webpack.js";
export type {
  WebpackPlugin,
  WebpackPluginFunction,
  WebpackPluginInstance
} from "./webpack.js";
export type {
  CacheOptions,
  ExperimentsOptions,
  FilesystemCacheOptions,
  InfrastructureLoggingLevel,
  InfrastructureLoggingOptions,
  MemoryCacheOptions,
  Mode,
  ModuleOptions,
  ModuleRule,
  ModuleRuleType,
  OptimizationOptions,
  ResolveOptions,
  SnapshotOptions,
  SnapshotPathPattern,
  SnapshotStrategyOptions,
  UnpackOptions
} from "./config/normalization.js";
export type { Chunk } from "./Chunk.js";
export type { ChunkGraph } from "./ChunkGraph.js";
export type { ChunkGroup } from "./ChunkGroup.js";
export type {
  Compilation,
  CompilationAsset,
  CompilationHooks,
  FinishModulesHook,
  ProcessAssetsHook
} from "./Compilation.js";
export type {
  CloseCallback,
  CompilationHook,
  Compiler,
  CompilerHooks,
  DoneHook,
  RunCallback
} from "./Compiler.js";
export type { Dependency } from "./Dependency.js";
export type { Entrypoint } from "./Entrypoint.js";
export type { ExportInfo, ExportsInfo } from "./ExportsInfo.js";
export type { Module } from "./Module.js";
export type { ModuleGraph } from "./ModuleGraph.js";
export type { ModuleGraphConnection } from "./ModuleGraphConnection.js";
export type {
  Stats,
  StatsAsset,
  StatsError,
  StatsJson,
  WatchDependencySets
} from "./Stats.js";
export type { TapOptions } from "./util.js";
export type {
  WatchHandler,
  WatchIgnored,
  WatchOptions,
  Watching
} from "./Watching.js";
