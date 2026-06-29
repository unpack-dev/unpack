import { createRequire } from "node:module";
import { isAbsolute, resolve } from "node:path";

export interface UnpackOptions {
  context?: string;
  entry: string | Record<string, string>;
  output?: {
    path?: string;
  };
  cache?: CacheOptions;
  snapshot?: SnapshotOptions;
}

export type CacheOptions =
  | boolean
  | {
      type?: "memory" | "filesystem";
      cacheDirectory?: string;
      cacheLocation?: string;
      name?: string;
      version?: string;
      buildDependencies?: Record<string, string[]>;
      maxMemoryGenerations?: number;
      idleTimeout?: number;
    };

export interface SnapshotOptions {
  module?: SnapshotStrategyOptions;
  buildDependencies?: SnapshotStrategyOptions;
}

export interface SnapshotStrategyOptions {
  timestamp?: boolean;
  hash?: boolean;
}

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
}

export interface Stats {
  hasErrors(): boolean;
  toJson(): StatsJson;
}

export interface Compiler {
  run(callback: RunCallback): void;
  close(callback: CloseCallback): void;
}

export type RunCallback = (err: Error | null, stats?: Stats) => void;
export type CloseCallback = (err: Error | null) => void;

interface NormalizedEntry {
  name: string;
  request: string;
}

interface NormalizedOptions {
  context: string;
  entries: NormalizedEntry[];
  outputPath: string;
  cache: NormalizedCacheOptions;
  snapshot: NormalizedSnapshotOptions;
}

interface NormalizedCacheOptions {
  type: "disabled" | "memory" | "filesystem";
  cacheDirectory?: string;
  cacheLocation?: string;
  name?: string;
  version?: string;
  buildDependencies: NormalizedBuildDependency[];
  maxMemoryGenerations?: number;
  idleTimeout?: number;
}

interface NormalizedBuildDependency {
  name: string;
  files: string[];
}

interface NormalizedSnapshotOptions {
  module: NormalizedSnapshotStrategy;
  buildDependencies: NormalizedSnapshotStrategy;
}

interface NormalizedSnapshotStrategy {
  timestamp: boolean;
  hash: boolean;
}

interface NativeStatsJson {
  errors: StatsError[];
  warnings?: StatsError[];
  assets: StatsAsset[];
  outputPath?: string;
  output_path?: string;
}

interface NativeRunResult {
  error?: {
    name: string;
    message: string;
  } | null;
  stats?: NativeStatsJson | null;
}

interface NativeFlushResult {
  error?: {
    name: string;
    message: string;
  } | null;
}

interface NativeCompiler {
  run(): Promise<NativeRunResult>;
  flushCache(): Promise<NativeFlushResult>;
  close(): void;
}

interface NativeBinding {
  createCompiler(options: NormalizedOptions): NativeCompiler;
}

const require = createRequire(import.meta.url);
const native = require("./unpack_node.node") as NativeBinding;

class CompilerImpl implements Compiler {
  #closed = false;
  #running = false;
  #idleFlushTimer: ReturnType<typeof setTimeout> | undefined;
  #pendingCacheFlush: Promise<NativeFlushResult> | undefined;
  readonly #filesystemCache: boolean;
  readonly #cacheIdleTimeout: number;
  readonly #nativeCompiler: NativeCompiler;

  constructor(options: NormalizedOptions) {
    this.#nativeCompiler = native.createCompiler(options);
    this.#filesystemCache = options.cache.type === "filesystem";
    this.#cacheIdleTimeout = options.cache.idleTimeout ?? 0;
  }

  run(callback: RunCallback): void {
    assertFunction(callback, "callback");

    if (this.#closed) {
      defer(() => callback(namedError("CompilerClosedError", "compiler is closed")));
      return;
    }

    if (this.#running) {
      defer(() =>
        callback(namedError("ConcurrentRunError", "compiler is already running"))
      );
      return;
    }

    this.#running = true;
    let run: Promise<NativeRunResult>;
    try {
      run = this.#nativeCompiler.run();
    } catch (error) {
      this.#running = false;
      defer(() => callback(toError(error, "InfrastructureError")));
      return;
    }

    run.then(
      (result) => {
        this.#running = false;
        if (result.error) {
          callback(namedError(result.error.name, result.error.message));
          return;
        }

        this.#scheduleIdleCacheFlush();
        callback(null, new StatsImpl(normalizeNativeStats(result.stats)));
      },
      (error: unknown) => {
        this.#running = false;
        callback(toError(error, "InfrastructureError"));
      }
    );
  }

  close(callback: CloseCallback): void {
    assertFunction(callback, "callback");

    if (this.#running) {
      defer(() =>
        callback(
          namedError("CompilerRunningError", "compiler cannot close while running")
        )
      );
      return;
    }

    try {
      this.#clearIdleFlushTimer();
      this.#flushCacheNow().then((flushError) => {
        if (flushError) {
          callback(flushError);
          return;
        }

        try {
          this.#nativeCompiler.close();
          this.#closed = true;
          callback(null);
        } catch (error) {
          callback(toError(error, "InfrastructureError"));
        }
      });
    } catch (error) {
      defer(() => callback(toError(error, "InfrastructureError")));
    }
  }

  #scheduleIdleCacheFlush(): void {
    if (!this.#filesystemCache || this.#closed) {
      return;
    }

    this.#clearIdleFlushTimer();
    this.#idleFlushTimer = setTimeout(() => {
      this.#idleFlushTimer = undefined;
      void this.#flushCacheNow();
    }, this.#cacheIdleTimeout);
  }

  #clearIdleFlushTimer(): void {
    if (this.#idleFlushTimer !== undefined) {
      clearTimeout(this.#idleFlushTimer);
      this.#idleFlushTimer = undefined;
    }
  }

  async #flushCacheNow(): Promise<Error | null> {
    if (!this.#filesystemCache) {
      return null;
    }

    const flush = this.#pendingCacheFlush ?? this.#nativeCompiler.flushCache();
    this.#pendingCacheFlush = flush;

    try {
      const result = await flush;
      if (result.error) {
        return namedError(result.error.name, result.error.message);
      }
      return null;
    } catch (error) {
      return toError(error, "InfrastructureError");
    } finally {
      if (this.#pendingCacheFlush === flush) {
        this.#pendingCacheFlush = undefined;
      }
    }
  }
}

class StatsImpl implements Stats {
  constructor(private readonly json: StatsJson) {}

  hasErrors(): boolean {
    return this.json.errors.length > 0;
  }

  toJson(): StatsJson {
    return {
      errors: this.json.errors.map(cloneStatsError),
      warnings: this.json.warnings.map(cloneStatsError),
      assets: this.json.assets.map((asset) => ({ ...asset })),
      outputPath: this.json.outputPath
    };
  }
}

export default function unpack(
  options: UnpackOptions,
  callback?: RunCallback
): Compiler {
  if (callback !== undefined) {
    assertFunction(callback, "callback");
  }

  const compiler = new CompilerImpl(normalizeOptions(options));
  if (callback) {
    compiler.run((runErr, stats) => {
      compiler.close((closeErr) => {
        callback(runErr ?? closeErr, stats);
      });
    });
  }
  return compiler;
}

function normalizeOptions(options: UnpackOptions): NormalizedOptions {
  assertPlainObject(options, "options");
  assertKnownKeys(options, ["context", "entry", "output", "cache", "snapshot"], "options");

  const context =
    options.context === undefined
      ? process.cwd()
      : assertString(options.context, "options.context");
  const normalizedContext = resolve(process.cwd(), context);
  const output = options.output ?? {};
  assertPlainObject(output, "options.output");
  assertKnownKeys(output, ["path"], "options.output");

  const outputPathValue =
    output.path === undefined
      ? "dist"
      : assertString(output.path, "options.output.path");
  const outputPath = isAbsolute(outputPathValue)
    ? outputPathValue
    : resolve(normalizedContext, outputPathValue);

  return {
    context: normalizedContext,
    entries: normalizeEntry(options.entry),
    outputPath,
    cache: normalizeCacheOptions(options.cache, normalizedContext),
    snapshot: normalizeSnapshotOptions(options.snapshot)
  };
}

function normalizeEntry(entry: UnpackOptions["entry"]): NormalizedEntry[] {
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

function normalizeCacheOptions(
  cache: CacheOptions | undefined,
  context: string
): NormalizedCacheOptions {
  if (cache === undefined || cache === true) {
    return {
      type: "memory",
      buildDependencies: []
    };
  }

  if (cache === false) {
    return {
      type: "disabled",
      buildDependencies: []
    };
  }

  if (typeof cache !== "object" || cache === null || Array.isArray(cache)) {
    throw new TypeError("options.cache must be a boolean or an object");
  }

  assertKnownKeys(
    cache,
    [
      "type",
      "cacheDirectory",
      "cacheLocation",
      "name",
      "version",
      "buildDependencies",
      "maxMemoryGenerations",
      "idleTimeout"
    ],
    "options.cache"
  );

  const type = cache.type === undefined ? "memory" : assertCacheType(cache.type);
  const name =
    cache.name === undefined ? undefined : assertNonEmptyString(cache.name, "options.cache.name");
  const cacheDirectory =
    cache.cacheDirectory === undefined
      ? type === "filesystem"
        ? resolve(context, "node_modules/.cache/unpack")
        : undefined
      : normalizePath(cache.cacheDirectory, "options.cache.cacheDirectory", context);
  const cacheLocation =
    cache.cacheLocation === undefined
      ? type === "filesystem" && cacheDirectory
        ? resolve(cacheDirectory, name ?? "default")
        : undefined
      : normalizePath(cache.cacheLocation, "options.cache.cacheLocation", context);

  return {
    type,
    ...(cacheDirectory === undefined ? {} : { cacheDirectory }),
    ...(cacheLocation === undefined ? {} : { cacheLocation }),
    ...(name === undefined ? {} : { name }),
    ...(cache.version === undefined
      ? {}
      : { version: assertString(cache.version, "options.cache.version") }),
    buildDependencies: normalizeBuildDependencies(cache.buildDependencies, context),
    ...(cache.maxMemoryGenerations === undefined
      ? {}
      : {
          maxMemoryGenerations: assertNonNegativeInteger(
            cache.maxMemoryGenerations,
            "options.cache.maxMemoryGenerations"
          )
        }),
    ...(cache.idleTimeout === undefined
      ? {}
      : {
          idleTimeout: assertNonNegativeInteger(
            cache.idleTimeout,
            "options.cache.idleTimeout"
          )
        })
  };
}

function normalizeBuildDependencies(
  buildDependencies: Record<string, string[]> | undefined,
  context: string
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
      files: files.map((file, index) =>
        normalizePath(file, `options.cache.buildDependencies.${name}[${index}]`, context)
      )
    };
  });
}

function normalizeSnapshotOptions(
  snapshot: SnapshotOptions | undefined
): NormalizedSnapshotOptions {
  if (snapshot === undefined) {
    return {
      module: { timestamp: true, hash: false },
      buildDependencies: { timestamp: true, hash: true }
    };
  }

  assertPlainObject(snapshot, "options.snapshot");
  assertKnownKeys(snapshot, ["module", "buildDependencies"], "options.snapshot");

  return {
    module: normalizeSnapshotStrategy(snapshot.module, "options.snapshot.module", {
      timestamp: true,
      hash: false
    }),
    buildDependencies: normalizeSnapshotStrategy(
      snapshot.buildDependencies,
      "options.snapshot.buildDependencies",
      {
        timestamp: true,
        hash: true
      }
    )
  };
}

function normalizeSnapshotStrategy(
  strategy: unknown,
  name: string,
  defaults: NormalizedSnapshotStrategy
): NormalizedSnapshotStrategy {
  if (strategy === undefined) {
    return { ...defaults };
  }

  assertPlainObject(strategy, name);
  assertKnownKeys(strategy, ["timestamp", "hash"], name);
  return {
    timestamp:
      strategy.timestamp === undefined
        ? defaults.timestamp
        : assertBoolean(strategy.timestamp, `${name}.timestamp`),
    hash: strategy.hash === undefined ? defaults.hash : assertBoolean(strategy.hash, `${name}.hash`)
  };
}

function normalizeNativeStats(stats: NativeStatsJson | null | undefined): StatsJson {
  if (!stats) {
    return { errors: [], warnings: [], assets: [], outputPath: "" };
  }
  return {
    errors: stats.errors.map(cloneStatsError),
    warnings: (stats.warnings ?? []).map(cloneStatsError),
    assets: stats.assets.map((asset) => ({ ...asset })),
    outputPath: stats.outputPath ?? stats.output_path ?? ""
  };
}

function cloneStatsError(error: StatsError): StatsError {
  return {
    message: error.message,
    ...(error.path === undefined ? {} : { path: error.path }),
    ...(error.request === undefined ? {} : { request: error.request }),
    ...(error.issuer === undefined ? {} : { issuer: error.issuer }),
    ...(error.stack === undefined ? {} : { stack: error.stack })
  };
}

function assertKnownKeys(
  value: Record<string, unknown>,
  keys: string[],
  name: string
): void {
  const allowed = new Set(keys);
  const unknown = Object.keys(value).filter((key) => !allowed.has(key));
  if (unknown.length > 0) {
    throw new TypeError(`${name} contains unknown option '${unknown[0]}'`);
  }
}

function assertPlainObject(value: unknown, name: string): asserts value is Record<string, unknown> {
  if (
    typeof value !== "object" ||
    value === null ||
    Array.isArray(value)
  ) {
    throw new TypeError(`${name} must be an object`);
  }
}

function assertString(value: unknown, name: string): string {
  if (typeof value !== "string") {
    throw new TypeError(`${name} must be a string`);
  }
  return value;
}

function normalizePath(value: unknown, name: string, context: string): string {
  const path = assertString(value, name);
  return isAbsolute(path) ? path : resolve(context, path);
}

function assertCacheType(value: unknown): "memory" | "filesystem" {
  if (value !== "memory" && value !== "filesystem") {
    throw new TypeError("options.cache.type must be 'memory' or 'filesystem'");
  }
  return value;
}

function assertBoolean(value: unknown, name: string): boolean {
  if (typeof value !== "boolean") {
    throw new TypeError(`${name} must be a boolean`);
  }
  return value;
}

function assertNonNegativeInteger(value: unknown, name: string): number {
  if (
    typeof value !== "number" ||
    !Number.isInteger(value) ||
    value < 0
  ) {
    throw new TypeError(`${name} must be a non-negative integer`);
  }
  return value;
}

function assertNonEmptyString(value: unknown, name: string): string {
  const string = assertString(value, name);
  if (string.length === 0) {
    throw new TypeError(`${name} must not be empty`);
  }
  return string;
}

function assertFunction(value: unknown, name: string): asserts value is Function {
  if (typeof value !== "function") {
    throw new TypeError(`${name} must be a function`);
  }
}

function defer(callback: () => void): void {
  queueMicrotask(callback);
}

function namedError(name: string, message: string): Error {
  const error = new Error(message);
  error.name = name;
  return error;
}

function toError(error: unknown, name: string): Error {
  if (error instanceof Error) {
    error.name = error.name === "Error" ? name : error.name;
    return error;
  }
  return namedError(name, String(error));
}
