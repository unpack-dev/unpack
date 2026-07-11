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
  module?: ModuleOptions;
}

export interface ModuleOptions {
  rules: ModuleRule[];
}

export interface ModuleRule {
  test: RegExp;
  loader: string;
  options?: Record<string, unknown>;
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

export interface Compilation {
  readonly moduleGraph: ModuleGraph;
  readonly chunkGraph: ChunkGraph;
  readonly modules: ReadonlySet<Module>;
}

export interface Chunk {
  readonly id: string | number | null;
  readonly name?: string;
}

export interface Module {
  readonly resource: string;
  readonly type: string;
  readonly dependencies: readonly Dependency[];
  identifier(): string;
  readableIdentifier(): string;
  nameForCondition(): string;
}

export interface Dependency {
  readonly type: string;
  readonly request?: string;
  readonly weak: boolean;
  getResourceIdentifier(): string | null;
}

export interface ModuleGraphConnection {
  readonly originModule: Module | null;
  readonly resolvedOriginModule: Module | null;
  readonly dependency: Dependency;
  readonly module: Module;
  readonly resolvedModule: Module;
  readonly weak: boolean;
  readonly conditional: false;
  readonly active: boolean;
  readonly explanations: ReadonlySet<string>;
  getActiveState(runtime?: unknown): boolean;
  isActive(runtime?: unknown): boolean;
  isTargetActive(runtime?: unknown): boolean;
}

export interface ExportInfo {
  readonly name: string;
  readonly provided: boolean;
  getUsedName(): string;
}

export interface ExportsInfo {
  getProvidedExports(): string[];
  isExportProvided(exportName: string | readonly string[]): boolean;
  getExportInfo(exportName: string): ExportInfo;
  getReadOnlyExportInfo(exportName: string): ExportInfo;
  getUsedExports(runtime?: unknown): null;
}

export interface ModuleGraph {
  getResolvedModule(dependency: Dependency): Module | null;
  getConnection(dependency: Dependency): ModuleGraphConnection | undefined;
  getModule(dependency: Dependency): Module | null;
  getOrigin(dependency: Dependency): Module | null;
  getResolvedOrigin(dependency: Dependency): Module | null;
  getParentModule(dependency: Dependency): Module | undefined;
  getParentBlock(dependency: Dependency): undefined;
  getParentBlockIndex(dependency: Dependency): number;
  getIncomingConnections(module: Module): ReadonlySet<ModuleGraphConnection>;
  getOutgoingConnections(module: Module): ReadonlySet<ModuleGraphConnection>;
  getIncomingConnectionsByOriginModule(
    module: Module
  ): ReadonlyMap<Module | null, readonly ModuleGraphConnection[]>;
  getOutgoingConnectionsByModule(
    module: Module
  ): ReadonlyMap<Module, readonly ModuleGraphConnection[]> | undefined;
  getIssuer(module: Module): Module | null | undefined;
  getOptimizationBailout(module: Module): readonly string[];
  getProvidedExports(module: Module): string[];
  isExportProvided(
    module: Module,
    exportName: string | readonly string[]
  ): boolean;
  getExportsInfo(module: Module): ExportsInfo;
  getExportInfo(module: Module, exportName: string): ExportInfo;
  getReadOnlyExportInfo(module: Module, exportName: string): ExportInfo;
  getUsedExports(module: Module, runtime?: unknown): null;
  cached<TArgs extends unknown[], TResult>(
    fn: (moduleGraph: ModuleGraph, ...args: TArgs) => TResult,
    ...args: TArgs
  ): TResult;
}

export interface ChunkGraph {
  getModuleId(module: Module): string | number | null;
  getModuleChunksIterable(module: Module): Iterable<Chunk>;
  getOrderedModuleChunksIterable(
    module: Module,
    comparator: (left: Chunk, right: Chunk) => number
  ): Iterable<Chunk>;
  getModuleChunks(module: Module): readonly Chunk[];
  getNumberOfModuleChunks(module: Module): number;
  getNumberOfChunkModules(chunk: Chunk): number;
  getChunkModulesIterable(chunk: Chunk): Iterable<Module>;
  getOrderedChunkModulesIterable(
    chunk: Chunk,
    comparator: (left: Module, right: Module) => number
  ): Iterable<Module>;
  getChunkModules(chunk: Chunk): readonly Module[];
  getOrderedChunkModules(
    chunk: Chunk,
    comparator: (left: Module, right: Module) => number
  ): readonly Module[];
  isModuleInChunk(module: Module, chunk: Chunk): boolean;
}

export interface TapOptions {
  name: string;
  stage?: number;
  before?: string | string[];
}

export interface DoneHook {
  tap(options: string | TapOptions, callback: (stats: Stats) => void): void;
  tapAsync(
    options: string | TapOptions,
    callback: (stats: Stats, done: (error?: Error | null) => void) => void
  ): void;
  tapPromise(
    options: string | TapOptions,
    callback: (stats: Stats) => PromiseLike<void>
  ): void;
  callAsync(stats: Stats, callback: (error?: Error | null) => void): void;
  promise(stats: Stats): Promise<void>;
}

export interface CompilerHooks {
  readonly done: DoneHook;
}

export interface Compiler {
  readonly hooks: CompilerHooks;
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
  moduleRules: NormalizedModuleRule[];
}

interface NormalizedModuleRule {
  test: string;
  loader: string;
  options: string;
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
  compression?: "gzip" | "brotli";
  allowCollectingMemory?: boolean;
  idleTimeout?: number;
  idleTimeoutForInitialStore?: number;
  idleTimeoutAfterLargeChanges?: number;
  profile: boolean;
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

interface NativeCompilation {
  modules(): NativeModule[];
  incomingConnections(moduleHandle: number): NativeModuleGraphConnection[];
  outgoingConnections(moduleHandle: number): NativeModuleGraphConnection[];
  chunks(): NativeChunk[];
  chunkModules(chunkHandle: number): number[];
  moduleChunks(moduleHandle: number): number[];
  moduleId(moduleHandle: number): string | number | null;
}

interface NativeModule {
  handle: number;
  identifier: string;
  resource: string;
  type: string;
  providedExports: string[];
}

interface NativeModuleGraphConnection {
  handle: number;
  originModuleHandle?: number | null;
  moduleHandle: number;
  dependencyType?: string;
  dependency_type?: string;
  request?: string | null;
  weak: boolean;
  parentBlockIndex?: number;
  parent_block_index?: number;
}

interface NativeChunk {
  handle: number;
  name?: string | null;
  renderId?: string | number | null;
  render_id?: string | number | null;
}
interface NativeRunResult {
  error?: {
    name: string;
    message: string;
  } | null;
  stats?: NativeStatsJson | null;
  compilation?: NativeCompilation | null;
  logs?: InfrastructureLogEvent[] | null;
}

interface NativeFlushResult {
  error?: {
    name: string;
    message: string;
  } | null;
  logs?: InfrastructureLogEvent[] | null;
}

interface NativeCompiler {
  run(): Promise<NativeRunResult>;
  settleCache(): Promise<NativeFlushResult>;
  shutdown(): Promise<NativeFlushResult>;
  close(): void;
}

interface NativeBinding {
  createCompiler(
    options: NormalizedOptions,
    loaderRunner?: (
      loader: string,
      resource: string,
      source: string,
      options: string
    ) => Promise<string>
  ): NativeCompiler;
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

type LoaderFunction = (
  this: {
    resourcePath: string;
    rootContext: string;
    sourceMap: false;
    getOptions(): Record<string, unknown>;
    async(): (error: unknown, source?: unknown) => void;
  },
  source: string
) => unknown;

type LoaderState =
  | { failed: false; loader: LoaderFunction }
  | { failed: true; error: unknown };

class LoaderRuntime {
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

interface DoneTap {
  name: string;
  stage: number;
  before: Set<string>;
  run(stats: Stats): Promise<void>;
}

class DoneHookImpl implements DoneHook {
  readonly #taps: DoneTap[] = [];

  tap(options: string | TapOptions, callback: (stats: Stats) => void): void {
    assertFunction(callback, "callback");
    this.#insert(options, async (stats) => {
      callback(stats);
    });
  }

  tapAsync(
    options: string | TapOptions,
    callback: (stats: Stats, done: (error?: Error | null) => void) => void
  ): void {
    assertFunction(callback, "callback");
    this.#insert(
      options,
      (stats) =>
        new Promise<void>((resolve, reject) => {
          let completed = false;
          const done = (error?: Error | null): void => {
            if (completed) return;
            completed = true;
            if (error) reject(error);
            else resolve();
          };
          try {
            callback(stats, done);
          } catch (error) {
            done(toError(error, "HookError"));
          }
        })
    );
  }

  tapPromise(
    options: string | TapOptions,
    callback: (stats: Stats) => PromiseLike<void>
  ): void {
    assertFunction(callback, "callback");
    this.#insert(options, async (stats) => {
      await callback(stats);
    });
  }

  callAsync(stats: Stats, callback: (error?: Error | null) => void): void {
    assertFunction(callback, "callback");
    void this.promise(stats).then(
      () => callback(),
      (error: unknown) => callback(toError(error, "HookError"))
    );
  }

  async promise(stats: Stats): Promise<void> {
    for (const tap of this.#taps) {
      await tap.run(stats);
    }
  }

  #insert(
    options: string | TapOptions,
    run: (stats: Stats) => Promise<void>
  ): void {
    const normalized = normalizeTapOptions(options);
    const tap: DoneTap = { ...normalized, run };
    const before = new Set(tap.before);
    let index = this.#taps.length;
    while (index > 0) {
      const current = this.#taps[index - 1];
      if (before.has(current.name)) {
        before.delete(current.name);
        index -= 1;
        continue;
      }
      if (before.size > 0 || current.stage > tap.stage) {
        index -= 1;
        continue;
      }
      break;
    }
    this.#taps.splice(index, 0, tap);
  }
}

function normalizeTapOptions(options: string | TapOptions): Omit<DoneTap, "run"> {
  if (typeof options === "string") {
    return { name: assertNonEmptyString(options, "options"), stage: 0, before: new Set() };
  }
  assertPlainObject(options, "options");
  const name = assertNonEmptyString(options.name, "options.name");
  const stage = options.stage === undefined ? 0 : options.stage;
  if (typeof stage !== "number" || !Number.isFinite(stage)) {
    throw new TypeError("options.stage must be a finite number");
  }
  const before = options.before === undefined
    ? []
    : typeof options.before === "string"
      ? [options.before]
      : options.before;
  if (!Array.isArray(before) || before.some((item) => typeof item !== "string")) {
    throw new TypeError("options.before must be a string or an array of strings");
  }
  return { name, stage, before: new Set(before) };
}

type CompilerLifecycle =
  | { kind: "open" }
  | { kind: "closing"; operation: Promise<Error | null> }
  | { kind: "closed" };

class CompilerImpl implements Compiler {
  readonly hooks: CompilerHooks = { done: new DoneHookImpl() };
  #lifecycle: CompilerLifecycle = { kind: "open" };
  #running = false;
  #watching: WatchingImpl | undefined;
  #idleFlushTimer: ReturnType<typeof setTimeout> | undefined;
  #pendingCacheFlush: Promise<NativeFlushResult> | undefined;
  #hasCompletedFilesystemCompilation = false;
  readonly #writableFilesystemCache: boolean;
  readonly #cacheIdleTimeout: number;
  readonly #cacheInitialStoreTimeout: number;
  readonly #cacheLargeChangeTimeout: number;
  readonly #infrastructureLoggingLevel: InfrastructureLoggingLevel;
  readonly #nativeCompiler: NativeCompiler;
  readonly #loaderRuntime: LoaderRuntime | undefined;

  constructor(options: NormalizedOptions) {
    this.#loaderRuntime = options.moduleRules.length > 0
      ? new LoaderRuntime(options.context)
      : undefined;
    this.#nativeCompiler = native.createCompiler(options, this.#loaderRuntime?.run);
    this.#writableFilesystemCache =
      options.cache.type === "filesystem" && !options.cache.readonly;
    this.#cacheIdleTimeout = options.cache.idleTimeout ?? 60_000;
    this.#cacheInitialStoreTimeout =
      options.cache.idleTimeoutForInitialStore ?? 5_000;
    this.#cacheLargeChangeTimeout =
      options.cache.idleTimeoutAfterLargeChanges ?? 1_000;
    this.#infrastructureLoggingLevel = options.infrastructureLogging.level;
  }

  run(callback: RunCallback): void {
    assertFunction(callback, "callback");

    if (this.#lifecycle.kind !== "open") {
      const error = namedError(
        "CompilerClosedError",
        `compiler is ${this.#lifecycle.kind}`
      );
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
      run = this.#runNativeCompilation();
    } catch (error) {
      const infrastructureError = toError(error, "InfrastructureError");
      this.#emitInfrastructureLog("error", "unpack.Compiler", infrastructureError.message);
      defer(() => {
        this.#running = false;
        callback(infrastructureError);
      });
      return;
    }

    run.then(
      (result) => {
        this.#emitInfrastructureLogs(result.logs);
        if (result.error) {
          const error = namedError(result.error.name, result.error.message);
          this.#emitInfrastructureLog("error", "unpack.Compiler", error.message);
          this.#deliverRunCallback(callback, error);
          return;
        }

        this.#scheduleIdleCacheFlush(
          this.#hasCompletedFilesystemCompilation
            ? this.#cacheIdleTimeout
            : this.#cacheInitialStoreTimeout
        );
        this.#hasCompletedFilesystemCompilation = true;
        this.#emitInfrastructureLog("info", "unpack.Compiler", "run completed");
        const stats = new StatsImpl(
          normalizeNativeStats(result.stats),
          result.compilation
        );
        void this.hooks.done.promise(stats).then(
          () => this.#deliverRunCallback(callback, null, stats),
          (error: unknown) =>
            this.#deliverRunCallback(callback, toError(error, "HookError"))
        );
      },
      (error: unknown) => {
        const infrastructureError = toError(error, "InfrastructureError");
        this.#emitInfrastructureLog("error", "unpack.Compiler", infrastructureError.message);
        this.#deliverRunCallback(callback, infrastructureError);
      }
    );
  }

  watch(watchOptions: WatchOptions, handler: WatchHandler): Watching {
    const normalizedWatchOptions = normalizeWatchOptions(watchOptions);
    assertFunction(handler, "handler");

    if (this.#lifecycle.kind !== "open") {
      const error = namedError(
        "CompilerClosedError",
        `compiler is ${this.#lifecycle.kind}`
      );
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

    if (this.#lifecycle.kind === "closed") {
      defer(() => callback(null));
      return;
    }

    if (this.#lifecycle.kind === "closing") {
      this.#deliverCloseCallback(this.#lifecycle.operation, callback);
      return;
    }

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

    const closeOperation = this.#closeNativeCompiler();
    this.#lifecycle = { kind: "closing", operation: closeOperation };
    this.#deliverCloseCallback(closeOperation, callback);
  }

  #deliverRunCallback(
    callback: RunCallback,
    error: Error | null,
    stats?: Stats
  ): void {
    this.#running = false;
    callback(error, stats);
  }

  #deliverCloseCallback(
    closeOperation: Promise<Error | null>,
    callback: CloseCallback
  ): void {
    void closeOperation.then(
      (error) => {
        defer(() => callback(error));
      },
      (error: unknown) => {
        defer(() => callback(toError(error, "InfrastructureError")));
      }
    );
  }

  async #closeNativeCompiler(): Promise<Error | null> {
    let closeError: Error | null = null;
    this.#clearIdleFlushTimer();
    await this.#flushCacheNow();

    try {
      const result = await this.#nativeCompiler.shutdown();
      this.#emitInfrastructureLogs(result.logs);
      if (result.error) {
        const error = namedError(result.error.name, result.error.message);
        this.#emitInfrastructureLog("warn", "unpack.Cache", error.message);
      }
    } catch (error) {
      closeError = toError(error, "InfrastructureError");
      this.#emitInfrastructureLog("error", "unpack.Compiler", closeError.message);
    }

    try {
      this.#nativeCompiler.close();
    } catch (error) {
      const infrastructureError = toError(error, "InfrastructureError");
      this.#emitInfrastructureLog("error", "unpack.Compiler", infrastructureError.message);
      closeError ??= infrastructureError;
    }

    this.#lifecycle = { kind: "closed" };
    return closeError;
  }

  async #runWatchCompilation(handler: WatchHandler): Promise<void> {
    let run: Promise<NativeRunResult>;
    try {
      this.#emitInfrastructureLog("info", "unpack.Watch", "watch compilation started");
      run = this.#runNativeCompilation();
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

      this.#scheduleIdleCacheFlush(
        this.#hasCompletedFilesystemCompilation
          ? this.#cacheLargeChangeTimeout
          : this.#cacheInitialStoreTimeout
      );
      this.#hasCompletedFilesystemCompilation = true;
      this.#emitInfrastructureLog("info", "unpack.Watch", "watch compilation completed");
      const stats = new StatsImpl(
        normalizeNativeStats(result.stats),
        result.compilation
      );
      try {
        await this.hooks.done.promise(stats);
      } catch (error) {
        handler(toError(error, "HookError"));
        return;
      }
      handler(null, stats);
    } catch (error) {
      const infrastructureError = toError(error, "InfrastructureError");
      this.#emitInfrastructureLog("error", "unpack.Watch", infrastructureError.message);
      handler(infrastructureError);
    }
  }

  #runNativeCompilation(): Promise<NativeRunResult> {
    this.#loaderRuntime?.beginCompilation();
    return this.#nativeCompiler.run();
  }

  #scheduleIdleCacheFlush(delay: number): void {
    if (!this.#writableFilesystemCache || this.#lifecycle.kind !== "open") return;
    this.#clearIdleFlushTimer();
    this.#idleFlushTimer = setTimeout(() => {
      this.#idleFlushTimer = undefined;
      void this.#flushCacheNow();
    }, delay);
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
    const flush = this.#pendingCacheFlush ?? this.#nativeCompiler.settleCache();
    this.#pendingCacheFlush = flush;

    try {
      const result = await flush;
      this.#emitInfrastructureLogs(result.logs);
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
    await this.#flushCache();
    this.#onClose();
    for (const callback of callbacks) {
      callback(null);
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
  readonly compilation: Compilation;
  readonly #json: StatsJson;

  constructor(json: StatsJson, compilation: NativeCompilation | null | undefined) {
    this.#json = json;
    this.compilation = new CompilationImpl(compilation);
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

class ModuleImpl implements Module {
  readonly #identifier: string;
  #moduleGraph: ModuleGraphImpl | undefined;
  #dependencies: readonly Dependency[] | undefined;

  constructor(
    readonly nativeHandle: number,
    readonly resource: string,
    readonly type: string,
    readonly providedExports: readonly string[],
    identifier: string
  ) {
    this.#identifier = identifier;
  }

  identifier(): string {
    return this.#identifier;
  }

  readableIdentifier(): string {
    return this.#identifier;
  }

  nameForCondition(): string {
    return this.resource;
  }

  get dependencies(): readonly Dependency[] {
    if (!this.#dependencies) {
      this.#dependencies = this.#moduleGraph
        ? [...this.#moduleGraph.getOutgoingConnections(this)].map(
            (connection) => connection.dependency
          )
        : [];
    }
    return this.#dependencies;
  }

  bindModuleGraph(moduleGraph: ModuleGraphImpl): void {
    this.#moduleGraph = moduleGraph;
  }
}

class DependencyImpl implements Dependency {
  constructor(
    readonly type: string,
    readonly request: string | undefined,
    readonly weak: boolean,
    readonly parentBlockIndex: number
  ) {}

  getResourceIdentifier(): string | null {
    return this.request === undefined ? null : `context|module${this.request}`;
  }
}

class ModuleGraphConnectionImpl implements ModuleGraphConnection {
  readonly resolvedOriginModule: Module | null;
  readonly resolvedModule: Module;
  readonly conditional = false as const;
  readonly active = true;
  readonly explanations: ReadonlySet<string> = new Set();

  constructor(
    readonly originModule: Module | null,
    readonly dependency: Dependency,
    readonly module: Module,
    readonly weak: boolean
  ) {
    this.resolvedOriginModule = originModule;
    this.resolvedModule = module;
  }

  getActiveState(_runtime?: unknown): boolean {
    return true;
  }

  isActive(_runtime?: unknown): boolean {
    return true;
  }

  isTargetActive(_runtime?: unknown): boolean {
    return true;
  }
}

class ExportInfoImpl implements ExportInfo {
  constructor(readonly name: string, readonly provided: boolean) {}

  getUsedName(): string {
    return this.name;
  }
}

class ExportsInfoImpl implements ExportsInfo {
  readonly #provided: Set<string>;
  readonly #exports = new Map<string, ExportInfo>();

  constructor(providedExports: readonly string[]) {
    this.#provided = new Set(providedExports);
  }

  getProvidedExports(): string[] {
    return [...this.#provided];
  }

  isExportProvided(exportName: string | readonly string[]): boolean {
    const name = typeof exportName === "string" ? exportName : exportName[0];
    return name !== undefined && this.#provided.has(name);
  }

  getExportInfo(exportName: string): ExportInfo {
    let info = this.#exports.get(exportName);
    if (!info) {
      info = new ExportInfoImpl(exportName, this.#provided.has(exportName));
      this.#exports.set(exportName, info);
    }
    return info;
  }

  getReadOnlyExportInfo(exportName: string): ExportInfo {
    return this.getExportInfo(exportName);
  }

  getUsedExports(_runtime?: unknown): null {
    return null;
  }
}

const EMPTY_CONNECTIONS: ReadonlySet<ModuleGraphConnection> = new Set();
const EMPTY_INCOMING_CONNECTION_GROUPS: ReadonlyMap<
  Module | null,
  readonly ModuleGraphConnection[]
> = new Map();
const EMPTY_OPTIMIZATION_BAILOUTS: readonly string[] = [];
const EMPTY_EXPORTS_INFO = new ExportsInfoImpl([]);

class ModuleGraphImpl implements ModuleGraph {
  readonly #nativeCompilation: NativeCompilation | undefined;
  readonly #modulesByHandle: ReadonlyMap<number, ModuleImpl>;
  readonly #connectionByHandle = new Map<number, ModuleGraphConnectionImpl>();
  readonly #connectionByDependency = new Map<Dependency, ModuleGraphConnectionImpl>();
  readonly #incoming = new Map<Module, Set<ModuleGraphConnection>>();
  readonly #outgoing = new Map<Module, Set<ModuleGraphConnection>>();
  readonly #incomingByOrigin = new Map<
    Module,
    ReadonlyMap<Module | null, readonly ModuleGraphConnection[]>
  >();
  readonly #outgoingByModule = new Map<
    Module,
    ReadonlyMap<Module, readonly ModuleGraphConnection[]>
  >();
  readonly #issuers = new Map<Module, Module | null>();
  readonly #loadedIncoming = new Set<Module>();
  readonly #loadedOutgoing = new Set<Module>();
  readonly #exports = new Map<Module, ExportsInfoImpl>();

  constructor(
    nativeCompilation: NativeCompilation | undefined,
    modulesByHandle: ReadonlyMap<number, ModuleImpl>
  ) {
    this.#nativeCompilation = nativeCompilation;
    this.#modulesByHandle = modulesByHandle;
    for (const module of modulesByHandle.values()) {
      this.#exports.set(module, new ExportsInfoImpl(module.providedExports));
    }
  }

  #materializeConnection(
    nativeConnection: NativeModuleGraphConnection
  ): ModuleGraphConnectionImpl | undefined {
    const existing = this.#connectionByHandle.get(nativeConnection.handle);
    if (existing) return existing;
    const target = this.#modulesByHandle.get(nativeConnection.moduleHandle);
    if (!target) return undefined;
    const originHandle = nativeConnection.originModuleHandle;
    const origin = originHandle == null
      ? null
      : this.#modulesByHandle.get(originHandle) ?? null;
    const dependency = new DependencyImpl(
      nativeConnection.dependencyType ?? nativeConnection.dependency_type ?? "unknown",
      nativeConnection.request ?? undefined,
      nativeConnection.weak,
      nativeConnection.parentBlockIndex ?? nativeConnection.parent_block_index ?? -1
    );
    const connection = new ModuleGraphConnectionImpl(
      origin,
      dependency,
      target,
      nativeConnection.weak
    );
    this.#connectionByHandle.set(nativeConnection.handle, connection);
    this.#connectionByDependency.set(dependency, connection);
    addToSetMap(this.#incoming, target, connection);
    if (origin) addToSetMap(this.#outgoing, origin, connection);
    return connection;
  }

  #loadIncoming(module: Module): void {
    if (this.#loadedIncoming.has(module)) return;
    this.#loadedIncoming.add(module);
    if (!(module instanceof ModuleImpl)) return;
    for (const connection of
      this.#nativeCompilation?.incomingConnections(module.nativeHandle) ?? []) {
      this.#materializeConnection(connection);
    }
  }

  #loadOutgoing(module: Module): void {
    if (this.#loadedOutgoing.has(module)) return;
    this.#loadedOutgoing.add(module);
    if (!(module instanceof ModuleImpl)) return;
    for (const connection of
      this.#nativeCompilation?.outgoingConnections(module.nativeHandle) ?? []) {
      this.#materializeConnection(connection);
    }
  }

  getResolvedModule(dependency: Dependency): Module | null {
    return this.getConnection(dependency)?.resolvedModule ?? null;
  }

  getConnection(dependency: Dependency): ModuleGraphConnectionImpl | undefined {
    return this.#connectionByDependency.get(dependency);
  }

  getModule(dependency: Dependency): Module | null {
    return this.getConnection(dependency)?.module ?? null;
  }

  getOrigin(dependency: Dependency): Module | null {
    return this.getConnection(dependency)?.originModule ?? null;
  }

  getResolvedOrigin(dependency: Dependency): Module | null {
    return this.getConnection(dependency)?.resolvedOriginModule ?? null;
  }

  getParentModule(dependency: Dependency): Module | undefined {
    return this.getConnection(dependency)?.originModule ?? undefined;
  }

  getParentBlock(_dependency: Dependency): undefined {
    return undefined;
  }

  getParentBlockIndex(dependency: Dependency): number {
    return dependency instanceof DependencyImpl ? dependency.parentBlockIndex : -1;
  }

  getIncomingConnections(module: Module): ReadonlySet<ModuleGraphConnection> {
    this.#loadIncoming(module);
    return this.#incoming.get(module) ?? EMPTY_CONNECTIONS;
  }

  getOutgoingConnections(module: Module): ReadonlySet<ModuleGraphConnection> {
    this.#loadOutgoing(module);
    return this.#outgoing.get(module) ?? EMPTY_CONNECTIONS;
  }

  getIncomingConnectionsByOriginModule(
    module: Module
  ): ReadonlyMap<Module | null, readonly ModuleGraphConnection[]> {
    let groups = this.#incomingByOrigin.get(module);
    if (!groups) {
      groups = groupConnections(
        this.getIncomingConnections(module),
        (connection) => connection.originModule
      );
      this.#incomingByOrigin.set(module, groups);
    }
    return groups ?? EMPTY_INCOMING_CONNECTION_GROUPS;
  }

  getOutgoingConnectionsByModule(
    module: Module
  ): ReadonlyMap<Module, readonly ModuleGraphConnection[]> | undefined {
    this.#loadOutgoing(module);
    const outgoing = this.#outgoing.get(module);
    if (!outgoing) return undefined;
    let groups = this.#outgoingByModule.get(module);
    if (!groups) {
      groups = groupConnections(outgoing, (connection) => connection.module);
      this.#outgoingByModule.set(module, groups);
    }
    return groups;
  }

  getIssuer(module: Module): Module | null | undefined {
    if (!this.#issuers.has(module)) {
      const incoming = this.getIncomingConnections(module);
      if (incoming.size === 0) return undefined;
      this.#issuers.set(
        module,
        [...incoming].find((connection) => connection.originModule !== null)
          ?.originModule ?? null
      );
    }
    return this.#issuers.get(module);
  }

  getOptimizationBailout(_module: Module): readonly string[] {
    return EMPTY_OPTIMIZATION_BAILOUTS;
  }

  getProvidedExports(module: Module): string[] {
    return this.getExportsInfo(module).getProvidedExports();
  }

  isExportProvided(
    module: Module,
    exportName: string | readonly string[]
  ): boolean {
    return this.getExportsInfo(module).isExportProvided(exportName);
  }

  getExportsInfo(module: Module): ExportsInfoImpl {
    return this.#exports.get(module) ?? EMPTY_EXPORTS_INFO;
  }

  getExportInfo(module: Module, exportName: string): ExportInfo {
    return this.getExportsInfo(module).getExportInfo(exportName);
  }

  getReadOnlyExportInfo(module: Module, exportName: string): ExportInfo {
    return this.getExportsInfo(module).getReadOnlyExportInfo(exportName);
  }

  getUsedExports(module: Module, runtime?: unknown): null {
    return this.getExportsInfo(module).getUsedExports(runtime);
  }

  cached<TArgs extends unknown[], TResult>(
    fn: (moduleGraph: ModuleGraph, ...args: TArgs) => TResult,
    ...args: TArgs
  ): TResult {
    return fn(this, ...args);
  }
}

class ChunkImpl implements Chunk {
  constructor(
    readonly nativeHandle: number,
    readonly id: string | number | null,
    readonly name: string | undefined
  ) {}
}

class SortableSetView<T> extends Set<T> {
  #lastComparator: ((left: T, right: T) => number) | undefined;

  sortWith(comparator: (left: T, right: T) => number): boolean {
    if (this.size <= 1 || comparator === this.#lastComparator) return false;
    const ordered = [...this].sort(comparator);
    super.clear();
    for (const value of ordered) super.add(value);
    this.#lastComparator = comparator;
    return true;
  }
}

const EMPTY_CHUNKS: readonly Chunk[] = [];
const EMPTY_MODULES: readonly Module[] = [];
const EMPTY_CHUNK_ITERABLE: SortableSetView<Chunk> = new SortableSetView();
const EMPTY_MODULE_ITERABLE: SortableSetView<Module> = new SortableSetView();

class ChunkGraphImpl implements ChunkGraph {
  readonly #nativeCompilation: NativeCompilation | undefined;
  readonly #modulesByHandle: ReadonlyMap<number, ModuleImpl>;
  readonly #chunksByHandle: ReadonlyMap<number, ChunkImpl>;
  readonly #moduleIds = new Map<Module, string | number | null>();
  readonly #moduleChunks = new Map<Module, readonly Chunk[]>();
  readonly #chunkModules = new Map<Chunk, readonly Module[]>();
  readonly #moduleChunkIterables = new Map<Module, SortableSetView<Chunk>>();
  readonly #chunkModuleIterables = new Map<Chunk, SortableSetView<Module>>();
  readonly #orderedChunkModules = new Map<
    Chunk,
    Map<(left: Module, right: Module) => number, readonly Module[]>
  >();

  constructor(
    nativeCompilation: NativeCompilation | undefined,
    modulesByHandle: ReadonlyMap<number, ModuleImpl>,
    chunksByHandle: ReadonlyMap<number, ChunkImpl>
  ) {
    this.#nativeCompilation = nativeCompilation;
    this.#modulesByHandle = modulesByHandle;
    this.#chunksByHandle = chunksByHandle;
  }

  #loadModuleChunks(module: Module): SortableSetView<Chunk> {
    const loaded = this.#moduleChunkIterables.get(module);
    if (loaded) return loaded;
    if (!(module instanceof ModuleImpl)) return EMPTY_CHUNK_ITERABLE;
    const chunks = (
      this.#nativeCompilation?.moduleChunks(module.nativeHandle) ?? []
    ).flatMap((handle) => {
        const chunk = this.#chunksByHandle.get(handle);
        return chunk ? [chunk] : [];
      });
    const iterable = new SortableSetView<Chunk>(chunks);
    this.#moduleChunkIterables.set(module, iterable);
    this.#moduleChunks.set(module, chunks);
    return iterable;
  }

  #loadChunkModules(chunk: Chunk): SortableSetView<Module> {
    const loaded = this.#chunkModuleIterables.get(chunk);
    if (loaded) return loaded;
    if (!(chunk instanceof ChunkImpl)) return EMPTY_MODULE_ITERABLE;
    const modules = (this.#nativeCompilation?.chunkModules(chunk.nativeHandle) ?? [])
      .flatMap((handle) => {
        const module = this.#modulesByHandle.get(handle);
        return module ? [module] : [];
      });
    const iterable = new SortableSetView<Module>(modules);
    this.#chunkModuleIterables.set(chunk, iterable);
    this.#chunkModules.set(chunk, modules);
    return iterable;
  }

  getModuleId(module: Module): string | number | null {
    if (!this.#moduleIds.has(module)) {
      const id = module instanceof ModuleImpl
        ? this.#nativeCompilation?.moduleId(module.nativeHandle) ?? null
        : null;
      this.#moduleIds.set(module, id);
    }
    return this.#moduleIds.get(module) ?? null;
  }

  getModuleChunksIterable(module: Module): Iterable<Chunk> {
    return this.#loadModuleChunks(module);
  }

  getOrderedModuleChunksIterable(
    module: Module,
    comparator: (left: Chunk, right: Chunk) => number
  ): Iterable<Chunk> {
    const chunks = this.#loadModuleChunks(module);
    if (chunks.sortWith(comparator)) {
      this.#moduleChunks.set(module, [...chunks]);
    }
    return chunks;
  }

  getModuleChunks(module: Module): readonly Chunk[] {
    this.#loadModuleChunks(module);
    return this.#moduleChunks.get(module) ?? EMPTY_CHUNKS;
  }

  getNumberOfModuleChunks(module: Module): number {
    return this.getModuleChunks(module).length;
  }

  getNumberOfChunkModules(chunk: Chunk): number {
    return this.getChunkModules(chunk).length;
  }

  getChunkModulesIterable(chunk: Chunk): Iterable<Module> {
    return this.#loadChunkModules(chunk);
  }

  getOrderedChunkModulesIterable(
    chunk: Chunk,
    comparator: (left: Module, right: Module) => number
  ): Iterable<Module> {
    const modules = this.#loadChunkModules(chunk);
    modules.sortWith(comparator);
    return modules;
  }

  getChunkModules(chunk: Chunk): readonly Module[] {
    this.#loadChunkModules(chunk);
    return this.#chunkModules.get(chunk) ?? EMPTY_MODULES;
  }

  getOrderedChunkModules(
    chunk: Chunk,
    comparator: (left: Module, right: Module) => number
  ): readonly Module[] {
    let orderedByComparator = this.#orderedChunkModules.get(chunk);
    if (!orderedByComparator) {
      orderedByComparator = new Map();
      this.#orderedChunkModules.set(chunk, orderedByComparator);
    }
    let ordered = orderedByComparator.get(comparator);
    if (!ordered) {
      ordered = [...this.getChunkModules(chunk)].sort(comparator);
      orderedByComparator.set(comparator, ordered);
    }
    return ordered;
  }

  isModuleInChunk(module: Module, chunk: Chunk): boolean {
    return this.#loadChunkModules(chunk).has(module);
  }
}

class CompilationImpl implements Compilation {
  readonly moduleGraph: ModuleGraph;
  readonly chunkGraph: ChunkGraph;
  readonly modules: ReadonlySet<Module>;

  constructor(compilation: NativeCompilation | null | undefined) {
    const modulesByHandle = new Map(
      (compilation?.modules() ?? []).map((module) => [
        module.handle,
        new ModuleImpl(
          module.handle,
          module.resource,
          module.type,
          module.providedExports,
          module.identifier
        )
      ])
    );
    const moduleGraph = new ModuleGraphImpl(compilation ?? undefined, modulesByHandle);
    for (const module of modulesByHandle.values()) {
      module.bindModuleGraph(moduleGraph);
    }
    const chunksByHandle = new Map(
      (compilation?.chunks() ?? []).map((chunk) => [
        chunk.handle,
        new ChunkImpl(
          chunk.handle,
          chunk.renderId ?? chunk.render_id ?? null,
          chunk.name ?? undefined
        )
      ])
    );
    this.moduleGraph = moduleGraph;
    this.chunkGraph = new ChunkGraphImpl(
      compilation ?? undefined,
      modulesByHandle,
      chunksByHandle
    );
    this.modules = new Set(modulesByHandle.values());
  }
}

function addToSetMap<TKey, TValue>(
  map: Map<TKey, Set<TValue>>,
  key: TKey,
  value: TValue
): void {
  let values = map.get(key);
  if (!values) {
    values = new Set();
    map.set(key, values);
  }
  values.add(value);
}

function groupConnections<TKey>(
  connections: Iterable<ModuleGraphConnection>,
  getKey: (connection: ModuleGraphConnection) => TKey
): ReadonlyMap<TKey, readonly ModuleGraphConnection[]> {
  const groups = new Map<TKey, ModuleGraphConnection[]>();
  for (const connection of connections) {
    const key = getKey(connection);
    const group = groups.get(key);
    if (group) group.push(connection);
    else groups.set(key, [connection]);
  }
  return groups;
}

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
    compiler = new CompilerImpl(normalizeOptions(options));
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
      "infrastructureLogging",
      "module"
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

  const sourcemap =
    options.sourcemap === undefined
      ? true
      : assertBoolean(options.sourcemap, "options.sourcemap");
  const moduleRules = normalizeModuleOptions(options.module);
  if (moduleRules.length > 0 && sourcemap) {
    throw new TypeError("options.sourcemap must be false when options.module.rules is configured");
  }

  return {
    context: normalizedContext,
    entries: normalizeEntry(options.entry),
    outputPath,
    sourcemap,
    cache: normalizeCacheOptions(options.cache, normalizedContext, mode, name),
    snapshot: normalizeSnapshotOptions(options.snapshot, mode),
    infrastructureLogging: normalizeInfrastructureLoggingOptions(options.infrastructureLogging),
    moduleRules
  };
}

function normalizeModuleOptions(module: ModuleOptions | undefined): NormalizedModuleRule[] {
  if (module === undefined) return [];
  assertPlainObject(module, "options.module");
  assertKnownKeys(module, ["rules"], "options.module");
  if (!Array.isArray(module.rules)) {
    throw new TypeError("options.module.rules must be an array");
  }
  return module.rules.map((rule, index) => {
    const name = `options.module.rules[${index}]`;
    assertPlainObject(rule, name);
    assertKnownKeys(rule, ["test", "loader", "options"], name);
    if (!(rule.test instanceof RegExp)) {
      throw new TypeError(`${name}.test must be a RegExp`);
    }
    if (rule.test.flags !== "") {
      throw new TypeError(`${name}.test must not use flags`);
    }
    const loader = assertString(rule.loader, `${name}.loader`);
    if (!isAbsolute(loader)) {
      throw new TypeError(`${name}.loader must be an absolute path`);
    }
    const options = rule.options ?? {};
    assertPlainObject(options, `${name}.options`);
    let serializedOptions: string;
    try {
      serializedOptions = JSON.stringify(options);
    } catch {
      throw new TypeError(`${name}.options must be JSON-serializable`);
    }
    return { test: rule.test.source, loader, options: serializedOptions };
  });
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
      profile: false,
      readonly: false
    };
  }

  if (cache === true) {
    return {
      type: "memory",
      buildDependencies: [],
      automaticBuildDependencies: [],
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
      profile: false,
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

function assertCacheCompression(value: unknown): "gzip" | "brotli" {
  if (value === "gzip" || value === "brotli") return value;
  throw new TypeError("options.cache.compression must be false, 'gzip', or 'brotli'");
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
