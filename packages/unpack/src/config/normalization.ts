// Organized to match webpack's lib/config/normalization.js responsibility.

import { statSync } from "node:fs";
import { dirname, isAbsolute, resolve } from "node:path";

import { unpackToolchainBuildDependencies } from "../binding.js";
import { assertBoolean, assertCacheCompression, assertCacheType, assertKnownKeys, assertMode, assertNonEmptyString, assertNonNegativeInteger, assertNonNegativeNumber, assertPlainObject, assertString, normalizeGenerationLimit, normalizePath } from "../util.js";
import type { WebpackPlugin } from "../webpack.js";

export interface UnpackOptions {
  name?: string;
  context?: string;
  mode?: Mode;
  entry: string | Record<string, string>;
  output?: {
    path?: string;
  };
  sourcemap?: boolean;
  cache?: CacheOptions;
  resolve?: ResolveOptions;
  snapshot?: SnapshotOptions;
  infrastructureLogging?: InfrastructureLoggingOptions;
  module?: ModuleOptions;
  optimization?: OptimizationOptions;
  experiments?: ExperimentsOptions;
  plugins?: WebpackPlugin[];
}

export interface ResolveOptions {
  cache?: boolean;
}

export interface ExperimentsOptions {
  cacheUnaffected?: boolean;
  serialRebuildMake?: boolean;
  unsafeWatchCacheInvalidation?: boolean;
}

export interface OptimizationOptions {
  concatenateModules?: boolean;
  providedExports?: boolean;
  usedExports?: boolean | "global";
  sideEffects?: boolean | "flag";
  splitChunks?: false | SplitChunksOptions;
}

export interface SplitChunksOptions {
  chunks?: "async";
  minChunks?: number;
  name?: string;
}

export interface ModuleOptions {
  rules: ModuleRule[];
}

export interface ModuleRule {
  test: RegExp;
  loader?: string;
  type?: ModuleRuleType;
  options?: Record<string, unknown>;
  sideEffects?: boolean;
}

export type ModuleRuleType =
  | "javascript/auto"
  | "json"
  | "asset"
  | "asset/resource"
  | "asset/inline"
  | "asset/source";

export type Mode = "development" | "production" | "none";

export type CacheOptions =
  | boolean
  | MemoryCacheOptions
  | FilesystemCacheOptions;

export interface MemoryCacheOptions {
  type: "memory";
  maxGenerations?: number;
  cacheUnaffected?: boolean;
}

export interface FilesystemCacheOptions {
  type: "filesystem";
  cacheDirectory?: string;
  cacheLocation?: string;
  name?: string;
  version?: string;
  buildDependencies?: Record<string, string[]>;
  maxMemoryGenerations?: number;
  memoryCacheUnaffected?: boolean;
  maxAge?: number;
  compression?: false | "gzip" | "brotli";
  allowCollectingMemory?: boolean;
  idleTimeout?: number;
  idleTimeoutForInitialStore?: number;
  idleTimeoutAfterLargeChanges?: number;
  profile?: boolean;
  readonly?: boolean;
  hashAlgorithm?: string;
  managedPaths?: SnapshotPathPattern[];
  immutablePaths?: SnapshotPathPattern[];
}

export interface SnapshotOptions {
  module?: SnapshotStrategyOptions;
  resolve?: SnapshotStrategyOptions;
  buildDependencies?: SnapshotStrategyOptions;
  resolveBuildDependencies?: SnapshotStrategyOptions;
  managedPaths?: SnapshotPathPattern[];
  immutablePaths?: SnapshotPathPattern[];
  unmanagedPaths?: SnapshotPathPattern[];
}

export interface SnapshotStrategyOptions {
  timestamp?: boolean;
  hash?: boolean;
}

export type SnapshotPathPattern = string | RegExp;

export interface InfrastructureLoggingOptions {
  level?: InfrastructureLoggingLevel;
}

export type InfrastructureLoggingLevel =
  | "none"
  | "error"
  | "warn"
  | "info"
  | "log"
  | "verbose";

export interface NormalizedEntry {
  name: string;
  request: string;
}

export interface NormalizedOptions {
  context: string;
  entries: NormalizedEntry[];
  outputPath: string;
  sourcemap: boolean;
  cache: NormalizedCacheOptions;
  snapshot: NormalizedSnapshotOptions;
  infrastructureLogging: NormalizedInfrastructureLoggingOptions;
  moduleRules: NormalizedModuleRule[];
  concatenateModules: boolean;
  providedExports: boolean;
  usedExports: boolean;
  sideEffects: "disabled" | "flag" | "analyze";
  splitChunks?: NormalizedSplitChunksOptions;
  resolveCache: boolean;
  serialRebuildMake: boolean;
  unsafeWatchCacheInvalidation: boolean;
}

export interface NormalizedSplitChunksOptions {
  chunks: "async";
  minChunks: number;
  name?: string;
}

export interface NormalizedModuleRule {
  test: string;
  loader?: string;
  type?: ModuleRuleType;
  options: string;
  sideEffects?: boolean;
}

export interface NormalizedCacheOptions {
  type: "disabled" | "memory" | "filesystem";
  cacheDirectory?: string;
  cacheLocation?: string;
  name?: string;
  version?: string;
  buildDependencies: NormalizedBuildDependency[];
  maxMemoryGenerations?: number;
  cacheUnaffected?: boolean;
  memoryCacheUnaffected?: boolean;
  automaticBuildDependencies: string[];
  maxAge?: number;
  compression?: "gzip" | "brotli";
  allowCollectingMemory?: boolean;
  idleTimeout?: number;
  idleTimeoutForInitialStore?: number;
  idleTimeoutAfterLargeChanges?: number;
  profile: boolean;
  readonly: boolean;
}

export interface NormalizedBuildDependency {
  name: string;
  requests: string[];
}

export interface NormalizedSnapshotOptions {
  module: NormalizedSnapshotStrategy;
  resolve: NormalizedSnapshotStrategy;
  buildDependencies: NormalizedSnapshotStrategy;
  resolveBuildDependencies: NormalizedSnapshotStrategy;
  managedPaths: NormalizedSnapshotPathPattern[];
  immutablePaths: NormalizedSnapshotPathPattern[];
  unmanagedPaths: NormalizedSnapshotPathPattern[];
}

export interface NormalizedSnapshotStrategy {
  timestamp: boolean;
  hash: boolean;
}

export type NormalizedSnapshotPathPattern =
  | {
      type: "path";
      path: string;
    }
  | {
      type: "regexp";
      source: string;
      flags: "" | "i";
    }
  | {
      type: "nodeModules";
    };

export interface NormalizedInfrastructureLoggingOptions {
  level: InfrastructureLoggingLevel;
}

export type InfrastructureLogEventLevel = Exclude<InfrastructureLoggingLevel, "none">;

export interface InfrastructureLogEvent {
  level: InfrastructureLogEventLevel;
  name: string;
  message: string;
}

export function normalizeOptions(options: UnpackOptions): NormalizedOptions {
  assertPlainObject(options, "options");
  assertKnownKeys(
    options,
    [
      "context", "name", "mode", "entry", "output", "sourcemap", "cache", "resolve",
      "snapshot", "infrastructureLogging", "module", "optimization",
      "experiments", "plugins"
    ],
    "options"
  );
  const context = options.context === undefined
    ? process.cwd()
    : assertString(options.context, "options.context");
  const mode = options.mode === undefined ? "production" : assertMode(options.mode);
  const experiments = normalizeExperimentsOptions(options.experiments);
  const cacheUnaffectedExperiment = experiments.cacheUnaffected;
  const name = options.name === undefined
    ? undefined
    : assertString(options.name, "options.name");
  const normalizedContext = resolve(process.cwd(), context);
  const output = options.output ?? {};
  assertPlainObject(output, "options.output");
  assertKnownKeys(output, ["path"], "options.output");
  const outputPathValue = output.path === undefined
    ? "dist"
    : assertString(output.path, "options.output.path");
  const outputPath = isAbsolute(outputPathValue)
    ? outputPathValue
    : resolve(normalizedContext, outputPathValue);
  const sourcemap = options.sourcemap === undefined
    ? true
    : assertBoolean(options.sourcemap, "options.sourcemap");
  const moduleRules = normalizeModuleOptions(options.module);
  const optimization = normalizeOptimizationOptions(options.optimization, mode);
  if (moduleRules.some((rule) => rule.loader !== undefined) && sourcemap) {
    throw new TypeError("options.sourcemap must be false when options.module.rules is configured");
  }
  const normalizedCache = normalizeCacheOptions(
    options.cache,
    normalizedContext,
    mode,
    name,
    cacheUnaffectedExperiment,
    experiments.unsafeWatchCacheInvalidation
  );
  return {
    context: normalizedContext,
    entries: normalizeEntry(options.entry),
    outputPath,
    sourcemap,
    cache: normalizedCache,
    snapshot: normalizeSnapshotOptions(options.snapshot, mode),
    infrastructureLogging: normalizeInfrastructureLoggingOptions(options.infrastructureLogging),
    moduleRules,
    concatenateModules: optimization.concatenateModules,
    providedExports: optimization.providedExports,
    usedExports: optimization.usedExports,
    sideEffects: optimization.sideEffects,
    splitChunks: optimization.splitChunks,
    resolveCache: normalizeResolveOptions(
      options.resolve,
      normalizedCache.type !== "disabled"
    ),
    serialRebuildMake: experiments.serialRebuildMake,
    unsafeWatchCacheInvalidation: experiments.unsafeWatchCacheInvalidation
  };
}

export function normalizeResolveOptions(
  resolveOptions: ResolveOptions | undefined,
  cacheEnabled: boolean
): boolean {
  if (resolveOptions === undefined) {
    return cacheEnabled;
  }
  assertPlainObject(resolveOptions, "options.resolve");
  assertKnownKeys(resolveOptions, ["cache"], "options.resolve");
  if (resolveOptions.cache === undefined) {
    return cacheEnabled;
  }
  const resolveCache = assertBoolean(
    resolveOptions.cache,
    "options.resolve.cache"
  );
  if (resolveCache && !cacheEnabled) {
    throw new TypeError("options.resolve.cache requires an enabled cache");
  }
  return resolveCache;
}

export function normalizeExperimentsOptions(
  experiments: ExperimentsOptions | undefined
): {
  cacheUnaffected: boolean;
  serialRebuildMake: boolean;
  unsafeWatchCacheInvalidation: boolean;
} {
  if (experiments === undefined) {
    return {
      cacheUnaffected: false,
      serialRebuildMake: true,
      unsafeWatchCacheInvalidation: false
    };
  }
  assertPlainObject(experiments, "options.experiments");
  assertKnownKeys(
    experiments,
    ["cacheUnaffected", "serialRebuildMake", "unsafeWatchCacheInvalidation"],
    "options.experiments"
  );
  return {
    cacheUnaffected:
      experiments.cacheUnaffected === undefined
        ? false
        : assertBoolean(
            experiments.cacheUnaffected,
            "options.experiments.cacheUnaffected"
          ),
    serialRebuildMake:
      experiments.serialRebuildMake === undefined
        ? true
        : assertBoolean(
            experiments.serialRebuildMake,
            "options.experiments.serialRebuildMake"
          ),
    unsafeWatchCacheInvalidation:
      experiments.unsafeWatchCacheInvalidation === undefined
        ? false
        : assertBoolean(
            experiments.unsafeWatchCacheInvalidation,
            "options.experiments.unsafeWatchCacheInvalidation"
          )
  };
}

export function normalizeOptimizationOptions(
  optimization: OptimizationOptions | undefined,
  mode: Mode
): { concatenateModules: boolean; providedExports: boolean; usedExports: boolean; sideEffects: "disabled" | "flag" | "analyze"; splitChunks?: NormalizedSplitChunksOptions } {
  if (optimization === undefined) {
    return {
      concatenateModules: mode === "production",
      providedExports: true,
      usedExports: mode === "production",
      sideEffects: mode === "production" ? "analyze" : "flag",
      splitChunks: undefined
    };
  }
  assertPlainObject(optimization, "options.optimization");
  assertKnownKeys(optimization, ["concatenateModules", "providedExports", "usedExports", "sideEffects", "splitChunks"], "options.optimization");
  return {
    concatenateModules: optimization.concatenateModules === undefined
      ? mode === "production"
      : assertBoolean(optimization.concatenateModules, "options.optimization.concatenateModules"),
    providedExports: optimization.providedExports === undefined
      ? true
      : assertBoolean(optimization.providedExports, "options.optimization.providedExports"),
    usedExports: optimization.usedExports === undefined
      ? mode === "production"
      : optimization.usedExports === "global"
        ? true
        : assertBoolean(optimization.usedExports, "options.optimization.usedExports"),
    sideEffects: optimization.sideEffects === undefined
      ? mode === "production" ? "analyze" : "flag"
      : optimization.sideEffects === "flag"
        ? "flag"
        : assertBoolean(optimization.sideEffects, "options.optimization.sideEffects")
          ? "analyze"
          : "disabled",
    splitChunks: normalizeSplitChunksOptions(optimization.splitChunks)
  };
}

function normalizeSplitChunksOptions(
  splitChunks: unknown
): NormalizedSplitChunksOptions | undefined {
  if (splitChunks === undefined || splitChunks === false) return undefined;
  assertPlainObject(splitChunks, "options.optimization.splitChunks");
  assertKnownKeys(splitChunks, ["chunks", "minChunks", "name"], "options.optimization.splitChunks");
  const options = splitChunks as unknown as SplitChunksOptions;
  if (options.chunks !== undefined && options.chunks !== "async") {
    throw new TypeError("options.optimization.splitChunks.chunks currently only supports 'async'");
  }
  const minChunks = options.minChunks === undefined
    ? 2
    : assertNonNegativeInteger(options.minChunks, "options.optimization.splitChunks.minChunks");
  if (minChunks < 1) {
    throw new TypeError("options.optimization.splitChunks.minChunks must be at least 1");
  }
  return {
    chunks: "async",
    minChunks,
    name: options.name === undefined
      ? undefined
      : assertNonEmptyString(options.name, "options.optimization.splitChunks.name")
  };
}

export function normalizeModuleOptions(module: ModuleOptions | undefined): NormalizedModuleRule[] {
  if (module === undefined) return [];
  assertPlainObject(module, "options.module");
  assertKnownKeys(module, ["rules"], "options.module");
  if (!Array.isArray(module.rules)) {
    throw new TypeError("options.module.rules must be an array");
  }
  return module.rules.map((rule, index) => {
    const name = `options.module.rules[${index}]`;
    assertPlainObject(rule, name);
    assertKnownKeys(rule, ["test", "loader", "type", "options", "sideEffects"], name);
    if (!(rule.test instanceof RegExp)) {
      throw new TypeError(`${name}.test must be a RegExp`);
    }
    if (rule.test.flags !== "") {
      throw new TypeError(`${name}.test must not use flags`);
    }
    const loader = rule.loader === undefined
      ? undefined
      : assertString(rule.loader, `${name}.loader`);
    if (loader !== undefined && !isAbsolute(loader)) {
      throw new TypeError(`${name}.loader must be an absolute path`);
    }
    const type = rule.type === undefined
      ? undefined
      : assertModuleRuleType(rule.type, `${name}.type`);
    const options = rule.options ?? {};
    assertPlainObject(options, `${name}.options`);
    let serializedOptions: string;
    try {
      serializedOptions = JSON.stringify(options);
    } catch {
      throw new TypeError(`${name}.options must be JSON-serializable`);
    }
    const sideEffects = rule.sideEffects === undefined
      ? undefined
      : assertBoolean(rule.sideEffects, `${name}.sideEffects`);
    return { test: rule.test.source, loader, type, options: serializedOptions, sideEffects };
  });
}

export function assertModuleRuleType(value: unknown, name: string): ModuleRuleType {
  if (
    value !== "javascript/auto" &&
    value !== "json" &&
    value !== "asset" &&
    value !== "asset/resource" &&
    value !== "asset/inline" &&
    value !== "asset/source"
  ) {
    throw new TypeError(`${name} must be a supported module type`);
  }
  return value;
}

export function normalizeEntry(entry: UnpackOptions["entry"]): NormalizedEntry[] {
  if (typeof entry === "string") {
    assertNonEmptyString(entry, "options.entry");
    return [{ name: "main", request: entry }];
  }

  assertPlainObject(entry, "options.entry");
  const entries = Object.entries(entry).map(([name, request]) => {
    assertNonEmptyString(name, "entry name");
    assertNonEmptyString(request, `options.entry.${name}`);
    return { name, request };
  });

  if (entries.length === 0) {
    throw new TypeError("options.entry must define at least one entry");
  }

  return entries;
}

export function normalizeCacheOptions(
  cache: CacheOptions | undefined,
  context: string,
  mode: Mode,
  compilerName: string | undefined,
  cacheUnaffectedExperiment: boolean,
  unsafeWatchCacheInvalidation: boolean
): NormalizedCacheOptions {
  if (cache === undefined) {
    return {
      type:
        mode === "development" || unsafeWatchCacheInvalidation
          ? "memory"
          : "disabled",
      buildDependencies: [],
      automaticBuildDependencies: [],
      ...(mode === "development" && cacheUnaffectedExperiment
        ? { cacheUnaffected: true }
        : {}),
      profile: false,
      readonly: false
    };
  }

  if (cache === false && unsafeWatchCacheInvalidation) {
    throw new TypeError(
      "options.experiments.unsafeWatchCacheInvalidation requires an enabled cache"
    );
  }

  if (cache === true) {
    return {
      type: "memory",
      buildDependencies: [],
      automaticBuildDependencies: [],
      ...(mode === "development" && cacheUnaffectedExperiment
        ? { cacheUnaffected: true }
        : {}),
      profile: false,
      readonly: false
    };
  }

  if (cache === false) {
    return {
      type: "disabled",
      buildDependencies: [],
      automaticBuildDependencies: [],
      profile: false,
      readonly: false
    };
  }

  if (typeof cache !== "object" || cache === null || Array.isArray(cache)) {
    throw new TypeError("options.cache must be a boolean or an object");
  }

  if (cache.type === undefined) {
    throw new TypeError("options.cache.type is required");
  }
  const type = assertCacheType(cache.type);
  const cacheRecord = cache as unknown as Record<string, unknown>;

  if (type === "memory") {
    assertCacheKeysForType(
      cacheRecord,
      ["type", "maxGenerations", "cacheUnaffected"],
      "memory"
    );
    const memoryCache = cache as MemoryCacheOptions;
    if (memoryCache.cacheUnaffected === true && !cacheUnaffectedExperiment) {
      throw new TypeError(
        "'cache.cacheUnaffected: true' is only allowed when 'experiments.cacheUnaffected' is enabled"
      );
    }
    const maxMemoryGenerations =
      memoryCache.maxGenerations === undefined
        ? undefined
        : normalizeGenerationLimit(
            memoryCache.maxGenerations,
            "options.cache.maxGenerations",
            false
          );
    return {
      type: "memory",
      buildDependencies: [],
      automaticBuildDependencies: [],
      ...(maxMemoryGenerations === undefined ? {} : { maxMemoryGenerations }),
      ...(memoryCache.cacheUnaffected === undefined
        ? mode === "development" && cacheUnaffectedExperiment
          ? { cacheUnaffected: true }
          : {}
        : {
            cacheUnaffected: assertBoolean(
              memoryCache.cacheUnaffected,
              "options.cache.cacheUnaffected"
            )
          }),
      profile: false,
      readonly: false
    };
  }

  if ((process.versions as NodeJS.ProcessVersions & { pnp?: string }).pnp !== undefined) {
    throw new TypeError("Yarn Plug'n'Play is not supported by filesystem cache");
  }

  const filesystemCache = cache as FilesystemCacheOptions;
  if (
    filesystemCache.memoryCacheUnaffected === true &&
    !cacheUnaffectedExperiment
  ) {
    throw new TypeError(
      "'cache.memoryCacheUnaffected: true' is only allowed when 'experiments.cacheUnaffected' is enabled"
    );
  }
  assertCacheKeysForType(
    cacheRecord,
    [
      "type",
      "cacheDirectory",
      "cacheLocation",
      "name",
      "version",
      "buildDependencies",
      "maxMemoryGenerations",
      "memoryCacheUnaffected",
      "maxAge",
      "compression",
      "allowCollectingMemory",
      "idleTimeout",
      "idleTimeoutForInitialStore",
      "idleTimeoutAfterLargeChanges",
      "profile",
      "readonly",
      "hashAlgorithm",
      "managedPaths",
      "immutablePaths"
    ],
    "filesystem"
  );

  if (filesystemCache.hashAlgorithm !== undefined) {
    assertString(
      filesystemCache.hashAlgorithm,
      "options.cache.hashAlgorithm"
    );
  }
  validateInertCachePathPatterns(
    filesystemCache.managedPaths,
    "options.cache.managedPaths"
  );
  validateInertCachePathPatterns(
    filesystemCache.immutablePaths,
    "options.cache.immutablePaths"
  );

  const name =
    filesystemCache.name === undefined
      ? `${compilerName ?? "default"}-${mode}`
      : assertString(filesystemCache.name, "options.cache.name");
  const cacheDirectory =
    filesystemCache.cacheDirectory === undefined
      ? defaultFilesystemCacheDirectory()
      : normalizePath(
          filesystemCache.cacheDirectory,
          "options.cache.cacheDirectory",
          context,
          true
        );
  const cacheLocation =
    filesystemCache.cacheLocation === undefined
      ? type === "filesystem" && cacheDirectory
        ? resolve(cacheDirectory, name)
        : undefined
      : normalizePath(
          filesystemCache.cacheLocation,
          "options.cache.cacheLocation",
          context,
          true
        );
  const readonly =
    filesystemCache.readonly === undefined
      ? false
      : assertBoolean(filesystemCache.readonly, "options.cache.readonly");
  const maxMemoryGenerations =
    filesystemCache.maxMemoryGenerations === undefined
      ? mode === "development"
        ? 5
        : undefined
      : normalizeGenerationLimit(
          filesystemCache.maxMemoryGenerations,
          "options.cache.maxMemoryGenerations",
          true
        );

  return {
    type,
    ...(cacheDirectory === undefined ? {} : { cacheDirectory }),
    ...(cacheLocation === undefined ? {} : { cacheLocation }),
    ...(name === undefined ? {} : { name }),
    ...(filesystemCache.version === undefined
      ? {}
      : {
          version: assertString(
            filesystemCache.version,
            "options.cache.version"
          )
        }),
    buildDependencies: normalizeBuildDependencies(
      filesystemCache.buildDependencies
    ),
    automaticBuildDependencies: [...unpackToolchainBuildDependencies],
    ...(maxMemoryGenerations === undefined ? {} : { maxMemoryGenerations }),
    ...(filesystemCache.memoryCacheUnaffected === undefined
      ? mode === "development" && cacheUnaffectedExperiment
        ? { memoryCacheUnaffected: true }
        : {}
      : {
          memoryCacheUnaffected: assertBoolean(
            filesystemCache.memoryCacheUnaffected,
            "options.cache.memoryCacheUnaffected"
          )
        }),
    ...(filesystemCache.maxAge === undefined ? {} : { maxAge: assertNonNegativeNumber(filesystemCache.maxAge, "options.cache.maxAge") }),
    ...(filesystemCache.compression === undefined || filesystemCache.compression === false
      ? {}
      : { compression: assertCacheCompression(filesystemCache.compression) }),
    ...(filesystemCache.allowCollectingMemory === undefined
      ? {}
      : { allowCollectingMemory: assertBoolean(filesystemCache.allowCollectingMemory, "options.cache.allowCollectingMemory") }),
    ...(filesystemCache.idleTimeout === undefined
      ? {}
      : {
          idleTimeout: assertNonNegativeInteger(
            filesystemCache.idleTimeout,
            "options.cache.idleTimeout"
          )
        }),
    ...(filesystemCache.idleTimeoutForInitialStore === undefined ? {} : { idleTimeoutForInitialStore: assertNonNegativeInteger(filesystemCache.idleTimeoutForInitialStore, "options.cache.idleTimeoutForInitialStore") }),
    ...(filesystemCache.idleTimeoutAfterLargeChanges === undefined ? {} : { idleTimeoutAfterLargeChanges: assertNonNegativeInteger(filesystemCache.idleTimeoutAfterLargeChanges, "options.cache.idleTimeoutAfterLargeChanges") }),
    profile:
      filesystemCache.profile === undefined
        ? false
        : assertBoolean(filesystemCache.profile, "options.cache.profile"),
    readonly
  };
}

export function defaultFilesystemCacheDirectory(): string {
  const cwd = process.cwd();
  let directory = cwd;

  for (;;) {
    try {
      if (statSync(resolve(directory, "package.json")).isFile()) {
        return resolve(directory, "node_modules/.cache/unpack");
      }
    } catch {
      // Continue toward the filesystem root.
    }

    const parent = dirname(directory);
    if (parent === directory) {
      return resolve(cwd, ".cache/unpack");
    }
    directory = parent;
  }
}

export function validateInertCachePathPatterns(
  patterns: unknown,
  name: string
): void {
  if (patterns === undefined) {
    return;
  }
  if (!Array.isArray(patterns)) {
    throw new TypeError(`${name} must be an array`);
  }

  for (const [index, pattern] of patterns.entries()) {
    const patternName = `${name}[${index}]`;
    if (typeof pattern === "string") {
      if (!isAbsolute(pattern)) {
        throw new TypeError(`${patternName} must be an absolute path`);
      }
      continue;
    }
    if (!(pattern instanceof RegExp)) {
      throw new TypeError(`${patternName} must be a string or RegExp`);
    }
  }
}

export function assertCacheKeysForType(
  cache: Record<string, unknown>,
  allowedKeys: string[],
  type: "memory" | "filesystem"
): void {
  const allowed = new Set(allowedKeys);
  const key = Object.keys(cache).find((candidate) => !allowed.has(candidate));
  if (key === undefined) {
    return;
  }

  const filesystemKeys = new Set([
    "cacheDirectory",
    "cacheLocation",
    "name",
    "version",
    "buildDependencies",
    "maxMemoryGenerations",
    "maxAge",
    "idleTimeout",
    "idleTimeoutForInitialStore",
    "idleTimeoutAfterLargeChanges",
    "profile",
    "readonly",
    "hashAlgorithm",
    "managedPaths",
    "immutablePaths"
  ]);
  if (type === "memory" && filesystemKeys.has(key)) {
    throw new TypeError(`options.cache.${key} is only valid for filesystem cache`);
  }

  throw new TypeError(`options.cache contains unknown option '${key}'`);
}

export function normalizeBuildDependencies(
  buildDependencies: Record<string, string[]> | undefined
): NormalizedBuildDependency[] {
  if (buildDependencies === undefined) {
    return [];
  }

  assertPlainObject(buildDependencies, "options.cache.buildDependencies");
  return Object.entries(buildDependencies).map(([name, files]) => {
    if (!Array.isArray(files)) {
      throw new TypeError(`options.cache.buildDependencies.${name} must be an array`);
    }
    return {
      name,
      requests: files.map((file, index) =>
        assertNonEmptyString(
          file,
          `options.cache.buildDependencies.${name}[${index}]`
        )
      )
    };
  });
}

export function normalizeSnapshotOptions(
  snapshot: SnapshotOptions | undefined,
  mode: Mode
): NormalizedSnapshotOptions {
  const moduleAndResolveDefaults = defaultModuleAndResolveSnapshotStrategy(mode);

  if (snapshot === undefined) {
    return {
      module: { ...moduleAndResolveDefaults },
      resolve: { ...moduleAndResolveDefaults },
      buildDependencies: { timestamp: true, hash: true },
      resolveBuildDependencies: { timestamp: true, hash: true },
      managedPaths: defaultManagedPaths(),
      immutablePaths: [],
      unmanagedPaths: []
    };
  }

  assertPlainObject(snapshot, "options.snapshot");
  assertKnownKeys(
    snapshot,
    [
      "module",
      "resolve",
      "buildDependencies",
      "resolveBuildDependencies",
      "managedPaths",
      "immutablePaths",
      "unmanagedPaths"
    ],
    "options.snapshot"
  );

  return {
    module: normalizeSnapshotStrategy(
      snapshot.module,
      "options.snapshot.module",
      moduleAndResolveDefaults
    ),
    resolve: normalizeSnapshotStrategy(
      snapshot.resolve,
      "options.snapshot.resolve",
      moduleAndResolveDefaults
    ),
    buildDependencies: normalizeSnapshotStrategy(
      snapshot.buildDependencies,
      "options.snapshot.buildDependencies",
      {
        timestamp: true,
        hash: true
      }
    ),
    resolveBuildDependencies: normalizeSnapshotStrategy(
      snapshot.resolveBuildDependencies,
      "options.snapshot.resolveBuildDependencies",
      {
        timestamp: true,
        hash: true
      }
    ),
    managedPaths: normalizeSnapshotPathPatterns(
      snapshot.managedPaths,
      "options.snapshot.managedPaths",
      defaultManagedPaths()
    ),
    immutablePaths: normalizeSnapshotPathPatterns(
      snapshot.immutablePaths,
      "options.snapshot.immutablePaths",
      []
    ),
    unmanagedPaths: normalizeSnapshotPathPatterns(
      snapshot.unmanagedPaths,
      "options.snapshot.unmanagedPaths",
      []
    )
  };
}

export function defaultManagedPaths(): NormalizedSnapshotPathPattern[] {
  return [{ type: "nodeModules" }];
}

export function defaultModuleAndResolveSnapshotStrategy(mode: Mode): NormalizedSnapshotStrategy {
  return mode === "development" || mode === "none"
    ? { timestamp: true, hash: false }
    : { timestamp: true, hash: true };
}

export function normalizeSnapshotStrategy(
  strategy: unknown,
  name: string,
  defaults: NormalizedSnapshotStrategy
): NormalizedSnapshotStrategy {
  if (strategy === undefined) {
    return { ...defaults };
  }

  assertPlainObject(strategy, name);
  assertKnownKeys(strategy, ["timestamp", "hash"], name);
  const normalized = {
    timestamp:
      strategy.timestamp === undefined
        ? defaults.timestamp
        : assertBoolean(strategy.timestamp, `${name}.timestamp`),
    hash: strategy.hash === undefined ? defaults.hash : assertBoolean(strategy.hash, `${name}.hash`)
  };
  return normalized;
}

export function normalizeSnapshotPathPatterns(
  patterns: unknown,
  name: string,
  defaults: NormalizedSnapshotPathPattern[]
): NormalizedSnapshotPathPattern[] {
  if (patterns === undefined) {
    return defaults.map((pattern) => ({ ...pattern }));
  }

  if (!Array.isArray(patterns)) {
    throw new TypeError(`${name} must be an array`);
  }

  return patterns.map((pattern, index) => normalizeSnapshotPathPattern(pattern, `${name}[${index}]`));
}

export function normalizeSnapshotPathPattern(
  pattern: unknown,
  name: string
): NormalizedSnapshotPathPattern {
  if (typeof pattern === "string") {
    if (!isAbsolute(pattern)) {
      throw new TypeError(`${name} must be an absolute path`);
    }
    return { type: "path", path: pattern };
  }

  if (pattern instanceof RegExp) {
    if (pattern.flags !== "" && pattern.flags !== "i") {
      throw new TypeError(`${name} RegExp flags must be empty or 'i'`);
    }
    return { type: "regexp", source: pattern.source, flags: pattern.flags as "" | "i" };
  }

  throw new TypeError(`${name} must be a string or RegExp`);
}

export function normalizeInfrastructureLoggingOptions(
  infrastructureLogging: InfrastructureLoggingOptions | undefined
): NormalizedInfrastructureLoggingOptions {
  if (infrastructureLogging === undefined) {
    return { level: "none" };
  }

  assertPlainObject(infrastructureLogging, "options.infrastructureLogging");
  assertKnownKeys(infrastructureLogging, ["level"], "options.infrastructureLogging");
  return {
    level:
      infrastructureLogging.level === undefined
        ? "none"
        : assertInfrastructureLoggingLevel(infrastructureLogging.level)
  };
}

export function assertInfrastructureLoggingLevel(value: unknown): InfrastructureLoggingLevel {
  if (
    value !== "none" &&
    value !== "error" &&
    value !== "warn" &&
    value !== "info" &&
    value !== "log" &&
    value !== "verbose"
  ) {
    throw new TypeError(
      "options.infrastructureLogging.level must be 'none', 'error', 'warn', 'info', 'log', or 'verbose'"
    );
  }
  return value;
}
