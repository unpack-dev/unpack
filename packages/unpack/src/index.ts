import { createRequire } from "node:module";
import { statSync, watch as watchFileSystem } from "node:fs";
import { dirname, isAbsolute, resolve } from "node:path";
import { fileURLToPath } from "node:url";

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
  snapshot?: SnapshotOptions;
  infrastructureLogging?: InfrastructureLoggingOptions;
}

export type Mode = "development" | "production" | "none";

export type CacheOptions =
  | boolean
  | MemoryCacheOptions
  | FilesystemCacheOptions;

export interface MemoryCacheOptions {
  type: "memory";
  maxGenerations?: number;
}

export interface FilesystemCacheOptions {
  type: "filesystem";
  cacheDirectory?: string;
  cacheLocation?: string;
  name?: string;
  version?: string;
  buildDependencies?: Record<string, string[]>;
  maxMemoryGenerations?: number;
  maxAge?: number;
  idleTimeout?: number;
  idleTimeoutForInitialStore?: number;
  idleTimeoutAfterLargeChanges?: number;
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
  hasErrors(): boolean;
  toJson(): StatsJson;
}

export interface Compiler {
  run(callback: RunCallback): void;
  watch(watchOptions: WatchOptions, handler: WatchHandler): Watching;
  close(callback: CloseCallback): void;
}

export interface Watching {
  close(callback: CloseCallback): void;
  invalidate(): void;
}

export interface WatchOptions {
  aggregateTimeout?: number;
  ignored?: WatchIgnored;
  poll?: true | number;
}

export type WatchIgnored = string | RegExp | Array<string | RegExp>;

export type RunCallback = (err: Error | null, stats?: Stats) => void;
export type WatchHandler = (err: Error | null, stats?: Stats) => void;
export type CloseCallback = (err: Error | null) => void;

interface NormalizedEntry {
  name: string;
  request: string;
}

interface NormalizedOptions {
  context: string;
  entries: NormalizedEntry[];
  outputPath: string;
  sourcemap: boolean;
  cache: NormalizedCacheOptions;
  snapshot: NormalizedSnapshotOptions;
  infrastructureLogging: NormalizedInfrastructureLoggingOptions;
}

interface NormalizedCacheOptions {
  type: "disabled" | "memory" | "filesystem";
  cacheDirectory?: string;
  cacheLocation?: string;
  name?: string;
  version?: string;
  buildDependencies: NormalizedBuildDependency[];
  maxMemoryGenerations?: number;
  automaticBuildDependencies: string[];
  maxAge?: number;
  idleTimeout?: number;
  idleTimeoutForInitialStore?: number;
  idleTimeoutAfterLargeChanges?: number;
  readonly: boolean;
}

interface NormalizedBuildDependency {
  name: string;
  requests: string[];
}

interface NormalizedSnapshotOptions {
  module: NormalizedSnapshotStrategy;
  resolve: NormalizedSnapshotStrategy;
  buildDependencies: NormalizedSnapshotStrategy;
  resolveBuildDependencies: NormalizedSnapshotStrategy;
  managedPaths: NormalizedSnapshotPathPattern[];
  immutablePaths: NormalizedSnapshotPathPattern[];
  unmanagedPaths: NormalizedSnapshotPathPattern[];
}

interface NormalizedSnapshotStrategy {
  timestamp: boolean;
  hash: boolean;
}

type NormalizedSnapshotPathPattern =
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

interface NormalizedInfrastructureLoggingOptions {
  level: InfrastructureLoggingLevel;
}

type InfrastructureLogEventLevel = Exclude<InfrastructureLoggingLevel, "none">;

interface InfrastructureLogEvent {
  level: InfrastructureLogEventLevel;
  name: string;
  message: string;
}

interface NormalizedWatchOptions {
  aggregateTimeout: number;
  ignored: WatchIgnoredMatcher[];
  pollInterval: number | undefined;
}

type WatchIgnoredMatcher =
  | {
      type: "path";
      value: string;
    }
  | {
      type: "regexp";
      value: RegExp;
    };

interface WatchSubscription {
  close(): void;
}

interface WatchTarget {
  path: string;
  kind: "file" | "context" | "missing";
}

interface PollSnapshot {
  exists: boolean;
  mtimeMs: number;
  size: number;
}

interface NativeStatsJson {
  errors: StatsError[];
  warnings?: StatsError[];
  assets: StatsAsset[];
  outputPath?: string;
  output_path?: string;
  watchDependencies?: WatchDependencySets;
  watch_dependencies?: WatchDependencySets;
}

interface NativeRunResult {
  error?: {
    name: string;
    message: string;
  } | null;
  stats?: NativeStatsJson | null;
  logs?: InfrastructureLogEvent[] | null;
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
const unpackJavaScriptPath = fileURLToPath(import.meta.url);
// The native addon is the compiled closure of the Rust compiler, parser, and
// resolver; together with the JS entry and package metadata it is the runtime toolchain.
const unpackToolchainBuildDependencies = [
  unpackJavaScriptPath,
  require.resolve("./unpack_node.node"),
  resolve(dirname(unpackJavaScriptPath), "../package.json")
];

class CompilerImpl implements Compiler {
  #closed = false;
  #running = false;
  #watching: WatchingImpl | undefined;
  #idleFlushTimer: ReturnType<typeof setTimeout> | undefined;
  #pendingCacheFlush: Promise<NativeFlushResult> | undefined;
  readonly #writableFilesystemCache: boolean;
  readonly #cacheIdleTimeout: number;
  readonly #infrastructureLoggingLevel: InfrastructureLoggingLevel;
  readonly #nativeCompiler: NativeCompiler;

  constructor(options: NormalizedOptions) {
    this.#nativeCompiler = native.createCompiler(options);
    this.#writableFilesystemCache =
      options.cache.type === "filesystem" && !options.cache.readonly;
    this.#cacheIdleTimeout = options.cache.idleTimeout ?? 0;
    this.#infrastructureLoggingLevel = options.infrastructureLogging.level;
  }

  run(callback: RunCallback): void {
    assertFunction(callback, "callback");

    if (this.#closed) {
      const error = namedError("CompilerClosedError", "compiler is closed");
      this.#emitInfrastructureLog("error", "unpack.Compiler", error.message);
      defer(() => callback(error));
      return;
    }

    if (this.#running) {
      const error = namedError("ConcurrentRunError", "compiler is already running");
      this.#emitInfrastructureLog("error", "unpack.Compiler", error.message);
      defer(() => callback(error));
      return;
    }

    if (this.#watching) {
      const error = namedError("ConcurrentRunError", "compiler is already watching");
      this.#emitInfrastructureLog("error", "unpack.Compiler", error.message);
      defer(() => callback(error));
      return;
    }

    this.#running = true;
    let run: Promise<NativeRunResult>;
    try {
      this.#emitInfrastructureLog("info", "unpack.Compiler", "run started");
      run = this.#nativeCompiler.run();
    } catch (error) {
      this.#running = false;
      const infrastructureError = toError(error, "InfrastructureError");
      this.#emitInfrastructureLog("error", "unpack.Compiler", infrastructureError.message);
      defer(() => callback(infrastructureError));
      return;
    }

    run.then(
      (result) => {
        this.#running = false;
        this.#emitInfrastructureLogs(result.logs);
        if (result.error) {
          const error = namedError(result.error.name, result.error.message);
          this.#emitInfrastructureLog("error", "unpack.Compiler", error.message);
          callback(error);
          return;
        }

        this.#scheduleIdleCacheFlush();
        this.#emitInfrastructureLog("info", "unpack.Compiler", "run completed");
        callback(null, new StatsImpl(normalizeNativeStats(result.stats)));
      },
      (error: unknown) => {
        this.#running = false;
        const infrastructureError = toError(error, "InfrastructureError");
        this.#emitInfrastructureLog("error", "unpack.Compiler", infrastructureError.message);
        callback(infrastructureError);
      }
    );
  }

  watch(watchOptions: WatchOptions, handler: WatchHandler): Watching {
    const normalizedWatchOptions = normalizeWatchOptions(watchOptions);
    assertFunction(handler, "handler");

    if (this.#closed) {
      const error = namedError("CompilerClosedError", "compiler is closed");
      this.#emitInfrastructureLog("error", "unpack.Watch", error.message);
      const watching = new WatchingImpl(
        (watchHandler) => {
          defer(() => watchHandler(error));
        },
        () => Promise.resolve(null),
        () => {},
        defaultWatchOptions()
      );
      watching.start(handler);
      return watching;
    }

    if (this.#running || this.#watching) {
      const error = namedError("ConcurrentRunError", "compiler is already running");
      this.#emitInfrastructureLog("error", "unpack.Watch", error.message);
      const watching = new WatchingImpl(
        (watchHandler) => {
          defer(() => watchHandler(error));
        },
        () => Promise.resolve(null),
        () => {},
        defaultWatchOptions()
      );
      watching.start(handler);
      return watching;
    }

    const watching = new WatchingImpl(
      (watchHandler) => this.#runWatchCompilation(watchHandler),
      () => this.#flushCacheNow(),
      () => {
        if (this.#watching === watching) {
          this.#watching = undefined;
        }
      },
      normalizedWatchOptions
    );
    this.#watching = watching;
    watching.start(handler);
    return watching;
  }

  close(callback: CloseCallback): void {
    assertFunction(callback, "callback");

    if (this.#running) {
      const error = namedError(
        "CompilerRunningError",
        "compiler cannot close while running"
      );
      this.#emitInfrastructureLog("error", "unpack.Compiler", error.message);
      defer(() => callback(error));
      return;
    }

    if (this.#watching) {
      const error = namedError(
        "CompilerRunningError",
        "compiler cannot close while watching"
      );
      this.#emitInfrastructureLog("error", "unpack.Compiler", error.message);
      defer(() => callback(error));
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
          const infrastructureError = toError(error, "InfrastructureError");
          this.#emitInfrastructureLog("error", "unpack.Compiler", infrastructureError.message);
          callback(infrastructureError);
        }
      });
    } catch (error) {
      const infrastructureError = toError(error, "InfrastructureError");
      this.#emitInfrastructureLog("error", "unpack.Compiler", infrastructureError.message);
      defer(() => callback(infrastructureError));
    }
  }

  async #runWatchCompilation(handler: WatchHandler): Promise<void> {
    let run: Promise<NativeRunResult>;
    try {
      this.#emitInfrastructureLog("info", "unpack.Watch", "watch compilation started");
      run = this.#nativeCompiler.run();
    } catch (error) {
      const infrastructureError = toError(error, "InfrastructureError");
      this.#emitInfrastructureLog("error", "unpack.Watch", infrastructureError.message);
      handler(infrastructureError);
      return;
    }

    try {
      const result = await run;
      this.#emitInfrastructureLogs(result.logs);
      if (result.error) {
        const error = namedError(result.error.name, result.error.message);
        this.#emitInfrastructureLog("error", "unpack.Watch", error.message);
        handler(error);
        return;
      }

      this.#scheduleIdleCacheFlush();
      this.#emitInfrastructureLog("info", "unpack.Watch", "watch compilation completed");
      handler(null, new StatsImpl(normalizeNativeStats(result.stats)));
    } catch (error) {
      const infrastructureError = toError(error, "InfrastructureError");
      this.#emitInfrastructureLog("error", "unpack.Watch", infrastructureError.message);
      handler(infrastructureError);
    }
  }

  #scheduleIdleCacheFlush(): void {
    if (!this.#writableFilesystemCache || this.#closed) {
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
    if (!this.#writableFilesystemCache) {
      return null;
    }

    const startsFlush = this.#pendingCacheFlush === undefined;
    if (startsFlush) {
      this.#emitInfrastructureLog("info", "unpack.Cache", "cache flush started");
    }
    const flush = this.#pendingCacheFlush ?? this.#nativeCompiler.flushCache();
    this.#pendingCacheFlush = flush;

    try {
      const result = await flush;
      if (result.error) {
        const error = namedError(result.error.name, result.error.message);
        if (startsFlush) {
          this.#emitInfrastructureLog("warn", "unpack.Cache", error.message);
        }
        return error;
      }
      if (startsFlush) {
        this.#emitInfrastructureLog("info", "unpack.Cache", "cache flush completed");
      }
      return null;
    } catch (error) {
      const infrastructureError = toError(error, "InfrastructureError");
      if (startsFlush) {
        this.#emitInfrastructureLog("error", "unpack.Cache", infrastructureError.message);
      }
      return infrastructureError;
    } finally {
      if (this.#pendingCacheFlush === flush) {
        this.#pendingCacheFlush = undefined;
      }
    }
  }

  #emitInfrastructureLog(
    level: InfrastructureLogEventLevel,
    name: string,
    message: string
  ): void {
    emitInfrastructureLog(
      { level, name, message },
      this.#infrastructureLoggingLevel
    );
  }

  #emitInfrastructureLogs(logs: InfrastructureLogEvent[] | null | undefined): void {
    if (!logs) {
      return;
    }
    for (const log of logs) {
      emitInfrastructureLog(log, this.#infrastructureLoggingLevel);
    }
  }
}

class WatchingImpl implements Watching {
  #closed = false;
  #running = false;
  #invalidated = false;
  #handler: WatchHandler | undefined;
  #rebuildTimer: ReturnType<typeof setTimeout> | undefined;
  readonly #watchers: WatchSubscription[] = [];
  readonly #runCompilation: (handler: WatchHandler) => Promise<void> | void;
  readonly #flushCache: () => Promise<Error | null>;
  readonly #onClose: () => void;
  readonly #watchOptions: NormalizedWatchOptions;
  readonly #closeCallbacks: CloseCallback[] = [];

  constructor(
    runCompilation: (handler: WatchHandler) => Promise<void> | void,
    flushCache: () => Promise<Error | null>,
    onClose: () => void,
    watchOptions: NormalizedWatchOptions
  ) {
    this.#runCompilation = runCompilation;
    this.#flushCache = flushCache;
    this.#onClose = onClose;
    this.#watchOptions = watchOptions;
  }

  start(handler: WatchHandler): void {
    this.#handler = handler;
    this.#run();
  }

  invalidate(): void {
    if (this.#closed) {
      return;
    }

    this.#clearRebuildTimer();

    if (this.#running) {
      this.#invalidated = true;
      return;
    }

    this.#run();
  }

  close(callback: CloseCallback): void {
    assertFunction(callback, "callback");

    if (this.#closed && !this.#running) {
      defer(() => callback(null));
      return;
    }

    this.#closed = true;
    this.#invalidated = false;
    this.#clearRebuildTimer();
    this.#closeWatchers();
    this.#closeCallbacks.push(callback);

    if (!this.#running) {
      void this.#finishClose();
    }
  }

  async #run(): Promise<void> {
    if (this.#closed || this.#running || !this.#handler) {
      return;
    }

    this.#clearRebuildTimer();
    this.#closeWatchers();
    this.#running = true;
    let latestStats: Stats | undefined;
    await this.#runCompilation((err, stats) => {
      if (!err && stats) {
        latestStats = stats;
      }
      this.#handler?.(err, stats);
    });
    this.#running = false;

    if (!this.#closed && latestStats) {
      const latestJson = latestStats.toJson();
      this.#replaceWatchers(latestJson.watchDependencies, latestJson.outputPath);
    }

    if (this.#closed) {
      await this.#finishClose();
      return;
    }

    if (this.#invalidated) {
      this.#invalidated = false;
      await this.#run();
    }
  }

  async #finishClose(): Promise<void> {
    const callbacks = this.#closeCallbacks.splice(0);
    const flushError = await this.#flushCache();
    this.#onClose();
    for (const callback of callbacks) {
      callback(flushError);
    }
  }

  #replaceWatchers(dependencies: WatchDependencySets, outputPath: string): void {
    this.#closeWatchers();
    const targets = watchTargets(dependencies).filter(
      (target) => !isIgnoredWatchPath(target.path, this.#watchOptions.ignored)
    );

    if (this.#watchOptions.pollInterval !== undefined) {
      if (targets.length > 0) {
        this.#watchers.push(this.#createPollWatcher(targets));
      }
      return;
    }

    const directlyWatchedPaths = new Set(
      targets
        .filter((target) => target.kind !== "context")
        .map((target) => target.path)
    );
    const contextWatchedPaths = new Set(
      targets
        .filter((target) => target.kind === "context")
        .map((target) => target.path)
    );
    const targetSnapshots = new Map(
      targets.map((target) => [target.path, pollSnapshot(target.path)])
    );
    for (const target of targets) {
      try {
        this.#watchers.push(
          watchFileSystem(target.path, { persistent: false }, (_eventType, filename) => {
            if (target.kind === "context" && !filename && this.#watchOptions.ignored.length > 0) {
              return;
            }
            const changedPath =
              target.kind === "context" && filename
                ? resolve(target.path, filename.toString())
                : target.path;
            if (isOutputWatchPath(changedPath, outputPath)) {
              return;
            }
            if (target.kind === "context" && contextWatchedPaths.has(changedPath)) {
              return;
            }
            if (target.kind === "context" && directlyWatchedPaths.has(changedPath)) {
              return;
            }
            if (isIgnoredWatchPath(changedPath, this.#watchOptions.ignored)) {
              return;
            }
            if (target.kind !== "context") {
              const previous = targetSnapshots.get(target.path);
              const next = pollSnapshot(target.path);
              targetSnapshots.set(target.path, next);
              if (previous && pollSnapshotsEqual(previous, next)) {
                return;
              }
            }
            this.#queueRebuild();
          })
        );
      } catch {
        // Missing or unsupported watch targets are represented in stats but should not
        // make an otherwise successful compilation fail.
      }
    }
  }

  #createPollWatcher(targets: WatchTarget[]): WatchSubscription {
    const snapshots = new Map(
      targets.map((target) => [target.path, pollSnapshot(target.path)])
    );
    const interval = setInterval(() => {
      let changed = false;
      for (const target of targets) {
        if (isIgnoredWatchPath(target.path, this.#watchOptions.ignored)) {
          continue;
        }
        const previous = snapshots.get(target.path);
        const next = pollSnapshot(target.path);
        snapshots.set(target.path, next);
        if (!previous || !pollSnapshotsEqual(previous, next)) {
          changed = true;
        }
      }
      if (changed) {
        this.#queueRebuild();
      }
    }, this.#watchOptions.pollInterval);
    interval.unref?.();
    return {
      close: () => clearInterval(interval)
    };
  }

  #queueRebuild(): void {
    if (this.#closed) {
      return;
    }

    this.#clearRebuildTimer();
    this.#rebuildTimer = setTimeout(() => {
      this.#rebuildTimer = undefined;
      this.invalidate();
    }, this.#watchOptions.aggregateTimeout);
  }

  #clearRebuildTimer(): void {
    if (this.#rebuildTimer !== undefined) {
      clearTimeout(this.#rebuildTimer);
      this.#rebuildTimer = undefined;
    }
  }

  #closeWatchers(): void {
    while (this.#watchers.length > 0) {
      this.#watchers.pop()?.close();
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
      outputPath: this.json.outputPath,
      watchDependencies: cloneWatchDependencies(this.json.watchDependencies)
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
  assertKnownKeys(
    options,
    [
      "context",
      "name",
      "mode",
      "entry",
      "output",
      "sourcemap",
      "cache",
      "snapshot",
      "infrastructureLogging"
    ],
    "options"
  );

  const context =
    options.context === undefined
      ? process.cwd()
      : assertString(options.context, "options.context");
  const mode = options.mode === undefined ? "production" : assertMode(options.mode);
  const name =
    options.name === undefined ? undefined : assertString(options.name, "options.name");
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
    sourcemap:
      options.sourcemap === undefined
        ? true
        : assertBoolean(options.sourcemap, "options.sourcemap"),
    cache: normalizeCacheOptions(options.cache, normalizedContext, mode, name),
    snapshot: normalizeSnapshotOptions(options.snapshot, mode),
    infrastructureLogging: normalizeInfrastructureLoggingOptions(options.infrastructureLogging)
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
  context: string,
  mode: Mode,
  compilerName: string | undefined
): NormalizedCacheOptions {
  if (cache === undefined) {
    return {
      type: mode === "development" ? "memory" : "disabled",
      buildDependencies: [],
      automaticBuildDependencies: [],
      readonly: false
    };
  }

  if (cache === true) {
    return {
      type: "memory",
      buildDependencies: [],
      automaticBuildDependencies: [],
      readonly: false
    };
  }

  if (cache === false) {
    return {
      type: "disabled",
      buildDependencies: [],
      automaticBuildDependencies: [],
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
    assertCacheKeysForType(cacheRecord, ["type", "maxGenerations"], "memory");
    const memoryCache = cache as MemoryCacheOptions;
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
      readonly: false
    };
  }

  if ((process.versions as NodeJS.ProcessVersions & { pnp?: string }).pnp !== undefined) {
    throw new TypeError("Yarn Plug'n'Play is not supported by filesystem cache");
  }

  const filesystemCache = cache as FilesystemCacheOptions;
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
      "maxAge",
      "idleTimeout",
      "idleTimeoutForInitialStore",
      "idleTimeoutAfterLargeChanges",
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
    ...(filesystemCache.maxAge === undefined ? {} : { maxAge: assertNonNegativeNumber(filesystemCache.maxAge, "options.cache.maxAge") }),
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
    readonly
  };
}

function defaultFilesystemCacheDirectory(): string {
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

function validateInertCachePathPatterns(
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

function assertCacheKeysForType(
  cache: Record<string, unknown>,
  allowedKeys: string[],
  type: "memory" | "filesystem"
): void {
  const allowed = new Set(allowedKeys);
  const key = Object.keys(cache).find((candidate) => !allowed.has(candidate));
  if (key === undefined) {
    return;
  }

  if (key === "cacheUnaffected" || key === "memoryCacheUnaffected") {
    throw new TypeError(`options.cache contains unsupported option '${key}'`);
  }

  const filesystemKeys = new Set([
    "cacheDirectory",
    "cacheLocation",
    "name",
    "version",
    "buildDependencies",
    "maxMemoryGenerations",
    "idleTimeout",
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

function normalizeBuildDependencies(
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

function normalizeSnapshotOptions(
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

function defaultManagedPaths(): NormalizedSnapshotPathPattern[] {
  return [{ type: "nodeModules" }];
}

function defaultModuleAndResolveSnapshotStrategy(mode: Mode): NormalizedSnapshotStrategy {
  return mode === "development" || mode === "none"
    ? { timestamp: true, hash: false }
    : { timestamp: true, hash: true };
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
  const normalized = {
    timestamp:
      strategy.timestamp === undefined
        ? defaults.timestamp
        : assertBoolean(strategy.timestamp, `${name}.timestamp`),
    hash: strategy.hash === undefined ? defaults.hash : assertBoolean(strategy.hash, `${name}.hash`)
  };
  return normalized;
}

function normalizeSnapshotPathPatterns(
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

function normalizeSnapshotPathPattern(
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

function normalizeInfrastructureLoggingOptions(
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

function assertInfrastructureLoggingLevel(value: unknown): InfrastructureLoggingLevel {
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

function normalizeWatchOptions(watchOptions: WatchOptions): NormalizedWatchOptions {
  assertPlainObject(watchOptions, "watchOptions");
  assertKnownKeys(watchOptions, ["aggregateTimeout", "ignored", "poll"], "watchOptions");
  return {
    aggregateTimeout:
      watchOptions.aggregateTimeout === undefined
        ? 20
        : assertNonNegativeInteger(watchOptions.aggregateTimeout, "watchOptions.aggregateTimeout"),
    ignored: normalizeWatchIgnored(watchOptions.ignored, "watchOptions.ignored"),
    pollInterval: normalizeWatchPoll(watchOptions.poll)
  };
}

function defaultWatchOptions(): NormalizedWatchOptions {
  return {
    aggregateTimeout: 20,
    ignored: [],
    pollInterval: undefined
  };
}

function normalizeWatchIgnored(value: unknown, name: string): WatchIgnoredMatcher[] {
  if (value === undefined) {
    return [];
  }

  if (Array.isArray(value)) {
    return value.map((item, index) => normalizeWatchIgnoredMatcher(item, `${name}[${index}]`));
  }

  return [normalizeWatchIgnoredMatcher(value, name)];
}

function normalizeWatchIgnoredMatcher(value: unknown, name: string): WatchIgnoredMatcher {
  if (typeof value === "string") {
    return {
      type: "path",
      value: normalizeWatchMatchPath(assertNonEmptyString(value, name))
    };
  }

  if (value instanceof RegExp) {
    return {
      type: "regexp",
      value
    };
  }

  throw new TypeError(`${name} must be a string or RegExp`);
}

function normalizeWatchPoll(value: unknown): number | undefined {
  if (value === undefined) {
    return undefined;
  }

  if (value === true) {
    return 500;
  }

  if (typeof value === "number") {
    return assertPositiveInteger(value, "watchOptions.poll");
  }

  throw new TypeError("watchOptions.poll must be true or a positive integer");
}

function normalizeNativeStats(stats: NativeStatsJson | null | undefined): StatsJson {
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

function cloneStatsError(error: StatsError): StatsError {
  return {
    message: error.message,
    ...(error.path === undefined ? {} : { path: error.path }),
    ...(error.request === undefined ? {} : { request: error.request }),
    ...(error.issuer === undefined ? {} : { issuer: error.issuer }),
    ...(error.stack === undefined ? {} : { stack: error.stack })
  };
}

function cloneWatchDependencies(dependencies: WatchDependencySets): WatchDependencySets {
  return {
    files: [...dependencies.files],
    contexts: [...dependencies.contexts],
    missing: [...dependencies.missing]
  };
}

function emptyWatchDependencies(): WatchDependencySets {
  return {
    files: [],
    contexts: [],
    missing: []
  };
}

function emitInfrastructureLog(
  event: InfrastructureLogEvent,
  configuredLevel: InfrastructureLoggingLevel
): void {
  if (!isInfrastructureLogEnabled(event.level, configuredLevel)) {
    return;
  }

  const message = `[${event.name}] ${event.message}`;
  switch (event.level) {
    case "error":
      console.error(message);
      return;
    case "warn":
      console.warn(message);
      return;
    case "info":
      console.info(message);
      return;
    case "log":
    case "verbose":
      console.log(message);
      return;
  }
}

function isInfrastructureLogEnabled(
  eventLevel: InfrastructureLogEventLevel,
  configuredLevel: InfrastructureLoggingLevel
): boolean {
  if (configuredLevel === "none") {
    return false;
  }
  return (
    infrastructureLogLevelRank(eventLevel) <=
    infrastructureLogLevelRank(configuredLevel)
  );
}

function infrastructureLogLevelRank(level: InfrastructureLogEventLevel): number {
  switch (level) {
    case "error":
      return 0;
    case "warn":
      return 1;
    case "info":
      return 2;
    case "log":
      return 3;
    case "verbose":
      return 4;
  }
}

function watchTargets(dependencies: WatchDependencySets): WatchTarget[] {
  const targets = new Map<string, WatchTarget>();
  for (const path of dependencies.files) {
    targets.set(path, { path, kind: "file" });
  }
  for (const path of dependencies.contexts) {
    targets.set(path, { path, kind: "context" });
  }
  for (const path of dependencies.missing) {
    targets.set(path, { path, kind: "missing" });
  }
  return [...targets.values()];
}

function isIgnoredWatchPath(path: string, ignored: WatchIgnoredMatcher[]): boolean {
  if (ignored.length === 0) {
    return false;
  }

  const normalizedPath = normalizeWatchMatchPath(path);
  return ignored.some((matcher) => {
    if (matcher.type === "path") {
      return normalizedPath === matcher.value || normalizedPath.includes(matcher.value);
    }

    matcher.value.lastIndex = 0;
    const matched = matcher.value.test(normalizedPath);
    matcher.value.lastIndex = 0;
    return matched;
  });
}

function isOutputWatchPath(path: string, outputPath: string): boolean {
  const normalizedPath = normalizeWatchMatchPath(path);
  const normalizedOutputPath = normalizeWatchMatchPath(outputPath);
  return (
    normalizedPath === normalizedOutputPath ||
    normalizedPath.startsWith(`${normalizedOutputPath}/`)
  );
}

function normalizeWatchMatchPath(path: string): string {
  const normalizedPath = path.replaceAll("\\", "/");
  return normalizedPath.startsWith("/private/var/")
    ? normalizedPath.replace(/^\/private\/var\//, "/var/")
    : normalizedPath;
}

function pollSnapshot(path: string): PollSnapshot {
  try {
    const stat = statSync(path);
    return {
      exists: true,
      mtimeMs: stat.mtimeMs,
      size: stat.size
    };
  } catch {
    return {
      exists: false,
      mtimeMs: 0,
      size: 0
    };
  }
}

function pollSnapshotsEqual(left: PollSnapshot, right: PollSnapshot): boolean {
  return left.exists === right.exists && left.mtimeMs === right.mtimeMs && left.size === right.size;
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

function normalizePath(
  value: unknown,
  name: string,
  context: string,
  requireAbsolute = false
): string {
  const path = assertString(value, name);
  if (requireAbsolute && !isAbsolute(path)) {
    throw new TypeError(`${name} must be an absolute path`);
  }
  return isAbsolute(path) ? path : resolve(context, path);
}

function assertCacheType(value: unknown): "memory" | "filesystem" {
  if (value !== "memory" && value !== "filesystem") {
    throw new TypeError("options.cache.type must be 'memory' or 'filesystem'");
  }
  return value;
}

function assertMode(value: unknown): Mode {
  if (value !== "development" && value !== "production" && value !== "none") {
    throw new TypeError("options.mode must be 'development', 'production', or 'none'");
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

function assertNonNegativeNumber(value: unknown, name: string): number {
  if (typeof value !== "number" || Number.isNaN(value) || value < 0) {
    throw new TypeError(`${name} must be a non-negative number`);
  }
  return value;
}
function normalizeGenerationLimit(
  value: unknown,
  name: string,
  allowZero: boolean
): number | undefined {
  const valid =
    typeof value === "number" &&
    !Number.isNaN(value) &&
    value !== Number.NEGATIVE_INFINITY &&
    (allowZero ? value >= 0 : value >= 1);
  if (!valid) {
    throw new TypeError(
      `${name} must be ${allowZero ? "non-negative" : "at least 1"}`
    );
  }
  if (value === Number.POSITIVE_INFINITY) {
    return undefined;
  }

  return Math.ceil(value);
}

function assertPositiveInteger(value: unknown, name: string): number {
  if (
    typeof value !== "number" ||
    !Number.isInteger(value) ||
    value <= 0
  ) {
    throw new TypeError(`${name} must be a positive integer`);
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
