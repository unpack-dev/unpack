// Organized to match webpack's lib/Stats.js responsibility.

import { NativeCompilation, NativeStatsJson } from "./binding.js";
import { Compilation, CompilationImpl } from "./Compilation.js";

export interface StatsError {
  message: string;
  path?: string;
  request?: string;
  issuer?: string;
  stack?: string;
}

export interface StatsAsset {
  name: string;
  size: number;
}

export interface StatsJson {
  errors: StatsError[];
  warnings: StatsError[];
  assets: StatsAsset[];
  outputPath: string;
  watchDependencies: WatchDependencySets;
}

export interface WatchDependencySets {
  files: string[];
  contexts: string[];
  missing: string[];
}

export interface Stats {
  readonly compilation: Compilation;
  hasErrors(): boolean;
  toJson(): StatsJson;
}

export function normalizeNativeStats(stats: NativeStatsJson | null | undefined): StatsJson {
  if (!stats) {
    return {
      errors: [],
      warnings: [],
      assets: [],
      outputPath: "",
      watchDependencies: emptyWatchDependencies()
    };
  }
  return {
    errors: stats.errors.map(cloneStatsError),
    warnings: (stats.warnings ?? []).map(cloneStatsError),
    assets: stats.assets.map((asset) => ({ ...asset })),
    outputPath: stats.outputPath ?? stats.output_path ?? "",
    watchDependencies: cloneWatchDependencies(
      stats.watchDependencies ?? stats.watch_dependencies ?? emptyWatchDependencies()
    )
  };
}

export function cloneStatsError(error: StatsError): StatsError {
  return {
    message: error.message,
    ...(error.path === undefined ? {} : { path: error.path }),
    ...(error.request === undefined ? {} : { request: error.request }),
    ...(error.issuer === undefined ? {} : { issuer: error.issuer }),
    ...(error.stack === undefined ? {} : { stack: error.stack })
  };
}

export function cloneWatchDependencies(dependencies: WatchDependencySets): WatchDependencySets {
  return {
    files: [...dependencies.files],
    contexts: [...dependencies.contexts],
    missing: [...dependencies.missing]
  };
}

export function emptyWatchDependencies(): WatchDependencySets {
  return { files: [], contexts: [], missing: [] };
}

export class StatsImpl implements Stats {
  readonly compilation: Compilation;
  readonly #json: StatsJson;

  constructor(
    json: StatsJson,
    compilation: NativeCompilation | null | undefined,
    existingCompilation?: Compilation
  ) {
    this.#json = json;
    this.compilation = existingCompilation ?? new CompilationImpl(compilation);
  }

  hasErrors(): boolean {
    return this.#json.errors.length > 0;
  }

  toJson(): StatsJson {
    return {
      errors: this.#json.errors.map(cloneStatsError),
      warnings: this.#json.warnings.map(cloneStatsError),
      assets: this.#json.assets.map((asset) => ({ ...asset })),
      outputPath: this.#json.outputPath,
      watchDependencies: cloneWatchDependencies(this.#json.watchDependencies)
    };
  }
}
