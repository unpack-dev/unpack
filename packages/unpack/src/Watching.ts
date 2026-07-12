// Organized to match webpack's lib/Watching.js responsibility.

import { statSync, watch as watchFileSystem } from "node:fs";
import { resolve } from "node:path";

import { CloseCallback } from "./Compiler.js";
import { Stats, WatchDependencySets } from "./Stats.js";
import { assertFunction, assertKnownKeys, assertNonEmptyString, assertNonNegativeInteger, assertPlainObject, assertPositiveInteger, defer } from "./util.js";

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

export type WatchHandler = (err: Error | null, stats?: Stats) => void;

export interface NormalizedWatchOptions {
  aggregateTimeout: number;
  ignored: WatchIgnoredMatcher[];
  pollInterval: number | undefined;
}

export type WatchIgnoredMatcher =
  | {
      type: "path";
      value: string;
    }
  | {
      type: "regexp";
      value: RegExp;
    };

export interface WatchSubscription {
  close(): void;
}

export interface WatchTarget {
  path: string;
  kind: "file" | "context" | "missing";
}

export interface PollSnapshot {
  exists: boolean;
  mtimeMs: number;
  size: number;
}

export class WatchingImpl implements Watching {
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

export function normalizeWatchOptions(watchOptions: WatchOptions): NormalizedWatchOptions {
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

export function defaultWatchOptions(): NormalizedWatchOptions {
  return {
    aggregateTimeout: 20,
    ignored: [],
    pollInterval: undefined
  };
}

export function normalizeWatchIgnored(value: unknown, name: string): WatchIgnoredMatcher[] {
  if (value === undefined) {
    return [];
  }

  if (Array.isArray(value)) {
    return value.map((item, index) => normalizeWatchIgnoredMatcher(item, `${name}[${index}]`));
  }

  return [normalizeWatchIgnoredMatcher(value, name)];
}

export function normalizeWatchIgnoredMatcher(value: unknown, name: string): WatchIgnoredMatcher {
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

export function normalizeWatchPoll(value: unknown): number | undefined {
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

export function watchTargets(dependencies: WatchDependencySets): WatchTarget[] {
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

export function isIgnoredWatchPath(path: string, ignored: WatchIgnoredMatcher[]): boolean {
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

export function isOutputWatchPath(path: string, outputPath: string): boolean {
  const normalizedPath = normalizeWatchMatchPath(path);
  const normalizedOutputPath = normalizeWatchMatchPath(outputPath);
  return (
    normalizedPath === normalizedOutputPath ||
    normalizedPath.startsWith(`${normalizedOutputPath}/`)
  );
}

export function normalizeWatchMatchPath(path: string): string {
  const normalizedPath = path.replaceAll("\\", "/");
  return normalizedPath.startsWith("/private/var/")
    ? normalizedPath.replace(/^\/private\/var\//, "/var/")
    : normalizedPath;
}

export function pollSnapshot(path: string): PollSnapshot {
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

export function pollSnapshotsEqual(left: PollSnapshot, right: PollSnapshot): boolean {
  return left.exists === right.exists && left.mtimeMs === right.mtimeMs && left.size === right.size;
}
