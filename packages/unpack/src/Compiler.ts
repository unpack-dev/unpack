// Organized to match webpack's lib/Compiler.js responsibility.

import { NativeCompilation, NativeCompiler, NativeFlushResult, NativeRunResult, NativeWatchChangeSet, native } from "./binding.js";
import { Compilation, CompilationImpl, ProcessAssetsHookImpl } from "./Compilation.js";
import { InfrastructureLogEvent, InfrastructureLogEventLevel, InfrastructureLoggingLevel, NormalizedOptions } from "./config/normalization.js";
import { LoaderRuntime } from "./LoaderRuntime.js";
import { Stats, StatsImpl, normalizeNativeStats } from "./Stats.js";
import { TapOptions, insertOrderedTap, normalizeTapOptions, assertFunction, defer, namedError, toError } from "./util.js";
import { WatchHandler, WatchOptions, Watching, WatchingImpl, defaultWatchOptions, normalizeWatchOptions } from "./Watching.js";

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

export interface CompilationHook {
  tap(options: string | TapOptions, callback: (compilation: Compilation) => void): void;
}

export interface CompilerHooks {
  readonly thisCompilation: CompilationHook;
  readonly compilation: CompilationHook;
  readonly done: DoneHook;
}

export interface Compiler {
  readonly hooks: CompilerHooks;
  readonly outputPath: string;
  run(callback: RunCallback): void;
  watch(watchOptions: WatchOptions, handler: WatchHandler): Watching;
  close(callback: CloseCallback): void;
}

export type RunCallback = (err: Error | null, stats?: Stats) => void;

export type CloseCallback = (err: Error | null) => void;

function emitInfrastructureLog(
  event: InfrastructureLogEvent,
  configuredLevel: InfrastructureLoggingLevel
): void {
  if (configuredLevel === "none" ||
      infrastructureLogLevelRank(event.level) > infrastructureLogLevelRank(configuredLevel)) {
    return;
  }
  const message = `[${event.name}] ${event.message}`;
  switch (event.level) {
    case "error": console.error(message); return;
    case "warn": console.warn(message); return;
    case "info": console.info(message); return;
    case "log":
    case "verbose": console.log(message); return;
  }
}

function infrastructureLogLevelRank(level: InfrastructureLogEventLevel): number {
  return ["error", "warn", "info", "log", "verbose"].indexOf(level);
}

export interface DoneTap {
  name: string;
  stage: number;
  before: Set<string>;
  run(stats: Stats): Promise<void>;
}

export class DoneHookImpl implements DoneHook {
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
    insertOrderedTap(this.#taps, tap);
  }
}

export class CompilationHookImpl implements CompilationHook {
  readonly #taps: Array<{
    name: string;
    stage: number;
    before: Set<string>;
    run(compilation: Compilation): void;
  }> = [];

  tap(options: string | TapOptions, callback: (compilation: Compilation) => void): void {
    assertFunction(callback, "callback");
    const tap = { ...normalizeTapOptions(options), run: callback };
    insertOrderedTap(this.#taps, tap);
  }

  call(compilation: Compilation): void {
    for (const tap of this.#taps) tap.run(compilation);
  }
}

export type CompilerLifecycle =
  | { kind: "open" }
  | { kind: "closing"; operation: Promise<Error | null> }
  | { kind: "closed" };

export class CompilerImpl implements Compiler {
  readonly hooks: CompilerHooks = {
    thisCompilation: new CompilationHookImpl(),
    compilation: new CompilationHookImpl(),
    done: new DoneHookImpl()
  };
  readonly outputPath: string;
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
  #nativeHookError: Error | undefined;
  #activeCompilation: CompilationImpl | undefined;

  constructor(options: NormalizedOptions) {
    this.outputPath = options.outputPath;
    this.#loaderRuntime = options.moduleRules.some((rule) => rule.loader !== undefined)
      ? new LoaderRuntime(options.context)
      : undefined;
    this.#nativeCompiler = native.createCompiler(
      options,
      this.#loaderRuntime?.run,
      async (nativeCompilation) => {
        try {
          const compilation = new CompilationImpl(nativeCompilation);
          this.#activeCompilation = compilation;
          (this.hooks.thisCompilation as CompilationHookImpl).call(compilation);
          (this.hooks.compilation as CompilationHookImpl).call(compilation);
        } catch (error) {
          this.#nativeHookError = toError(error, "HookError");
          throw this.#nativeHookError;
        }
      },
      async (nativeCompilation) => {
        const compilation = this.#activeCompilation ?? new CompilationImpl(nativeCompilation);
        try {
          compilation.update(nativeCompilation);
          await compilation.hooks.finishModules.promise(compilation.modules);
        } catch (error) {
          this.#nativeHookError = toError(error, "HookError");
          throw this.#nativeHookError;
        } finally {
          try {
            nativeCompilation.returnModuleGraphLease();
          } finally {
            compilation.releaseNativeCompilation();
          }
        }
      },
      async (nativeAssets) => {
        const compilation = this.#activeCompilation;
        let assetPhaseStarted = false;
        try {
          if (!compilation) {
            throw new Error("processAssets ran without an active Compilation");
          }
          compilation.update(nativeAssets.compilation());
          const hook = compilation.hooks.processAssets as ProcessAssetsHookImpl;
          if (hook.isUsed() || compilation.hasPendingAssetMutations()) {
            compilation.beginProcessAssets(nativeAssets.takeAssetSources());
            assetPhaseStarted = true;
            await hook.promise(compilation.assets);
            nativeAssets.replaceAssetSources(compilation.serializeAssets());
          }
        } catch (error) {
          this.#nativeHookError = toError(error, "HookError");
          throw this.#nativeHookError;
        } finally {
          try {
            nativeAssets.returnAssetsLease();
          } finally {
            if (assetPhaseStarted) compilation?.endProcessAssets();
            compilation?.releaseNativeCompilation();
          }
        }
      }
    );
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
          const error = this.#nativeHookError ?? namedError(result.error.name, result.error.message);
          this.#nativeHookError = undefined;
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
          result.compilation,
          this.#takeActiveCompilation(result.compilation)
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
      (watchHandler, isRebuild, watchChangeSet) =>
        this.#runWatchCompilation(watchHandler, isRebuild, watchChangeSet),
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

  async #runWatchCompilation(
    handler: WatchHandler,
    isRebuild: boolean,
    watchChangeSet?: NativeWatchChangeSet
  ): Promise<void> {
    let run: Promise<NativeRunResult>;
    try {
      this.#emitInfrastructureLog("info", "unpack.Watch", "watch compilation started");
      run = this.#runNativeCompilation(isRebuild, watchChangeSet);
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
        const error = this.#nativeHookError ?? namedError(result.error.name, result.error.message);
        this.#nativeHookError = undefined;
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
        result.compilation,
        this.#takeActiveCompilation(result.compilation)
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

  #runNativeCompilation(
    isRebuild = false,
    watchChangeSet?: NativeWatchChangeSet
  ): Promise<NativeRunResult> {
    this.#nativeHookError = undefined;
    this.#activeCompilation = undefined;
    this.#loaderRuntime?.beginCompilation();
    return this.#nativeCompiler.run({ isRebuild, watchChangeSet });
  }

  #takeActiveCompilation(
    nativeCompilation: NativeCompilation | null | undefined
  ): CompilationImpl | undefined {
    const compilation = this.#activeCompilation;
    this.#activeCompilation = undefined;
    compilation?.update(nativeCompilation);
    return compilation;
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
