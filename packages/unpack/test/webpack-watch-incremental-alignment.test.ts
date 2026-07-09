import { writeFile, utimes } from "node:fs/promises";
import { join } from "node:path";
import assert from "node:assert/strict";
import test from "node:test";

import webpack from "webpack";
import type {
  Compiler as WebpackCompiler,
  Stats as WebpackStats,
  Watching as WebpackWatching
} from "webpack";

import unpack from "@unpack-js/core";
import type {
  Compiler as UnpackCompiler,
  Stats as UnpackStats,
  Watching as UnpackWatching
} from "@unpack-js/core";

import {
  closeUnpackCompiler,
  closeWebpackCompiler,
  createComparisonFixture,
  readAsset,
  unpackOptions,
  webpackNodeOptions
} from "./webpack-comparison-helpers.js";

interface SharedWatchOptions {
  aggregateTimeout?: number;
  ignored?: RegExp;
  poll?: number;
}

interface StatsLike {
  hasErrors(): boolean;
}

interface WatchResult<TStats extends StatsLike> {
  err: Error | null;
  stats: TStats | undefined;
}

interface WatchSession<TStats extends StatsLike> {
  results: WatchResults<TStats>;
  close(): Promise<void>;
}

interface ClosableWatchSession {
  close(): Promise<void>;
}

test("watch comparison rebuilds changed source after watched file edits", async () => {
  const fixture = await createComparisonFixture("watch-edit-alignment-", {
    "src/index.js": "import { value } from './dep'; export const result = value;",
    "src/dep.js": "export const value = 'before';"
  });
  let webpackSession: WatchSession<WebpackStats> | undefined;
  let unpackSession: WatchSession<UnpackStats> | undefined;

  try {
    webpackSession = startWebpackWatch(fixture.webpackRoot, { aggregateTimeout: 0 });
    unpackSession = startUnpackWatch(fixture.unpackRoot, { aggregateTimeout: 0 });

    await assertSuccessfulCalls(
      [webpackSession.results.waitForCall(1, "webpack initial build")],
      [unpackSession.results.waitForCall(1, "Unpack initial build")]
    );
    await assertAssetContains(fixture.webpackRoot, "main.js", /before/);
    await assertAssetContains(fixture.unpackRoot, "main.js", /before/);
    const [webpackBaseline, unpackBaseline] = await settleWatchPair(
      webpackSession,
      unpackSession,
      "watched edit initial watch"
    );

    const webpackSecond = webpackSession.results.waitForCall(
      webpackBaseline + 1,
      "webpack watched rebuild"
    );
    const unpackSecond = unpackSession.results.waitForCall(
      unpackBaseline + 1,
      "Unpack watched rebuild"
    );
    await Promise.all([
      writeSource(fixture.webpackRoot, "src/dep.js", "export const value = 'after';"),
      writeSource(fixture.unpackRoot, "src/dep.js", "export const value = 'after';")
    ]);

    await assertSuccessfulCalls([webpackSecond], [unpackSecond]);
    assert.equal(webpackSession.results.calls, webpackBaseline + 1);
    assert.equal(unpackSession.results.calls, unpackBaseline + 1);
    await assertAssetContains(fixture.webpackRoot, "main.js", /after/);
    await assertAssetContains(fixture.unpackRoot, "main.js", /after/);
  } finally {
    await closeWatchSessions(webpackSession, unpackSession);
    await fixture.cleanup();
  }
});

test("watch comparison coalesces rapid edits with aggregateTimeout", async () => {
  const fixture = await createComparisonFixture("watch-aggregate-alignment-", {
    "src/index.js": "export const value = 'initial';"
  });
  let webpackSession: WatchSession<WebpackStats> | undefined;
  let unpackSession: WatchSession<UnpackStats> | undefined;

  try {
    webpackSession = startWebpackWatch(fixture.webpackRoot, { aggregateTimeout: 80 });
    unpackSession = startUnpackWatch(fixture.unpackRoot, { aggregateTimeout: 80 });

    await assertSuccessfulCalls(
      [webpackSession.results.waitForCall(1, "webpack initial build")],
      [unpackSession.results.waitForCall(1, "Unpack initial build")]
    );
    const [webpackBaseline, unpackBaseline] = await settleWatchPair(
      webpackSession,
      unpackSession,
      "aggregateTimeout initial watch"
    );

    const webpackSecond = webpackSession.results.waitForCall(
      webpackBaseline + 1,
      "webpack coalesced rebuild"
    );
    const unpackSecond = unpackSession.results.waitForCall(
      unpackBaseline + 1,
      "Unpack coalesced rebuild"
    );
    await Promise.all([
      writeSource(fixture.webpackRoot, "src/index.js", "export const value = 'first';"),
      writeSource(fixture.unpackRoot, "src/index.js", "export const value = 'first';")
    ]);
    await Promise.all([
      writeSource(fixture.webpackRoot, "src/index.js", "export const value = 'second';"),
      writeSource(fixture.unpackRoot, "src/index.js", "export const value = 'second';")
    ]);

    await assertSuccessfulCalls([webpackSecond], [unpackSecond]);
    await Promise.all([
      webpackSession.results.expectNoCallBeyond(
        webpackBaseline + 1,
        220,
        "webpack aggregateTimeout window"
      ),
      unpackSession.results.expectNoCallBeyond(
        unpackBaseline + 1,
        220,
        "Unpack aggregateTimeout window"
      )
    ]);
    assert.equal(webpackSession.results.calls, webpackBaseline + 1);
    assert.equal(unpackSession.results.calls, unpackBaseline + 1);
    await assertAssetContains(fixture.webpackRoot, "main.js", /second/);
    await assertAssetContains(fixture.unpackRoot, "main.js", /second/);
  } finally {
    await closeWatchSessions(webpackSession, unpackSession);
    await fixture.cleanup();
  }
});

test("watch comparison accepts numeric poll and rebuilds", async () => {
  const fixture = await createComparisonFixture("watch-poll-alignment-", {
    "src/index.js": "export const value = 'before';"
  });
  let webpackSession: WatchSession<WebpackStats> | undefined;
  let unpackSession: WatchSession<UnpackStats> | undefined;

  try {
    webpackSession = startWebpackWatch(fixture.webpackRoot, {
      aggregateTimeout: 0,
      poll: 30
    });
    unpackSession = startUnpackWatch(fixture.unpackRoot, {
      aggregateTimeout: 0,
      poll: 30
    });

    await assertSuccessfulCalls(
      [webpackSession.results.waitForCall(1, "webpack initial build")],
      [unpackSession.results.waitForCall(1, "Unpack initial build")]
    );
    await assertAssetContains(fixture.webpackRoot, "main.js", /before/);
    await assertAssetContains(fixture.unpackRoot, "main.js", /before/);
    const [webpackBaseline, unpackBaseline] = await settleWatchPair(
      webpackSession,
      unpackSession,
      "poll initial watch"
    );

    const webpackSecond = webpackSession.results.waitForCall(
      webpackBaseline + 1,
      "webpack polling rebuild"
    );
    const unpackSecond = unpackSession.results.waitForCall(
      unpackBaseline + 1,
      "Unpack polling rebuild"
    );
    await Promise.all([
      writeSource(fixture.webpackRoot, "src/index.js", "export const value = 'after';", {
        touchMtime: false
      }),
      writeSource(fixture.unpackRoot, "src/index.js", "export const value = 'after';", {
        touchMtime: false
      })
    ]);

    await assertSuccessfulCalls([webpackSecond], [unpackSecond]);
    assert.equal(webpackSession.results.calls, webpackBaseline + 1);
    assert.equal(unpackSession.results.calls, unpackBaseline + 1);
    await assertAssetContains(fixture.webpackRoot, "main.js", /after/);
    await assertAssetContains(fixture.unpackRoot, "main.js", /after/);
  } finally {
    await closeWatchSessions(webpackSession, unpackSession);
    await fixture.cleanup();
  }
});

test("watch comparison ignores RegExp-matched paths", async () => {
  const fixture = await createComparisonFixture("watch-ignored-alignment-", {
    "src/index.js":
      "import { value } from './ignored'; export const result = `index-before:${value}`;",
    "src/ignored.js": "export const value = 'ignored-before';"
  });
  let webpackSession: WatchSession<WebpackStats> | undefined;
  let unpackSession: WatchSession<UnpackStats> | undefined;

  try {
    webpackSession = startWebpackWatch(fixture.webpackRoot, {
      aggregateTimeout: 30,
      ignored: /ignored\.js$/
    });
    unpackSession = startUnpackWatch(fixture.unpackRoot, {
      aggregateTimeout: 30,
      ignored: /ignored\.js$/
    });

    await assertSuccessfulCalls(
      [webpackSession.results.waitForCall(1, "webpack initial build")],
      [unpackSession.results.waitForCall(1, "Unpack initial build")]
    );
    const [webpackBaseline, unpackBaseline] = await settleWatchPair(
      webpackSession,
      unpackSession,
      "ignored initial watch"
    );

    await Promise.all([
      writeSource(fixture.webpackRoot, "src/ignored.js", "export const value = 'ignored-after';"),
      writeSource(fixture.unpackRoot, "src/ignored.js", "export const value = 'ignored-after';")
    ]);
    await Promise.all([
      webpackSession.results.expectNoCallBeyond(
        webpackBaseline,
        220,
        "webpack ignored-path edit"
      ),
      unpackSession.results.expectNoCallBeyond(
        unpackBaseline,
        220,
        "Unpack ignored-path edit"
      )
    ]);
    assert.equal(webpackSession.results.calls, webpackBaseline);
    assert.equal(unpackSession.results.calls, unpackBaseline);

    const webpackSecond = webpackSession.results.waitForCall(
      webpackBaseline + 1,
      "webpack non-ignored rebuild"
    );
    const unpackSecond = unpackSession.results.waitForCall(
      unpackBaseline + 1,
      "Unpack non-ignored rebuild"
    );
    await Promise.all([
      writeSource(
        fixture.webpackRoot,
        "src/index.js",
        "import { value } from './ignored'; export const result = `index-after:${value}`;"
      ),
      writeSource(
        fixture.unpackRoot,
        "src/index.js",
        "import { value } from './ignored'; export const result = `index-after:${value}`;"
      )
    ]);

    await assertSuccessfulCalls([webpackSecond], [unpackSecond]);
    assert.equal(webpackSession.results.calls, webpackBaseline + 1);
    assert.equal(unpackSession.results.calls, unpackBaseline + 1);
    await assertAssetContains(fixture.webpackRoot, "main.js", /index-after/);
    await assertAssetContains(fixture.unpackRoot, "main.js", /index-after/);
  } finally {
    await closeWatchSessions(webpackSession, unpackSession);
    await fixture.cleanup();
  }
});

class WatchResults<TStats extends StatsLike> {
  readonly #results: Array<WatchResult<TStats>> = [];
  #waiters: Array<() => void> = [];

  readonly handler = (err: Error | null | undefined, stats?: TStats): void => {
    this.#results.push({ err: err ?? null, stats });
    const waiters = this.#waiters.splice(0);
    for (const waiter of waiters) {
      waiter();
    }
  };

  get calls(): number {
    return this.#results.length;
  }

  async waitForCall(count: number, label: string): Promise<WatchResult<TStats>> {
    const timeoutMs = 6_000;
    const deadline = Date.now() + timeoutMs;
    while (this.#results.length < count) {
      const remainingMs = deadline - Date.now();
      if (remainingMs <= 0) {
        break;
      }
      const changed = await this.#waitForNextCallback(remainingMs);
      if (!changed) {
        break;
      }
    }

    if (this.#results.length < count) {
      throw new Error(
        `${label}: timed out waiting for watch callback ${count}; observed ${this.#results.length}`
      );
    }

    return this.#results[count - 1]!;
  }

  async expectNoCallBeyond(count: number, quietMs: number, label: string): Promise<void> {
    assert.equal(
      this.#results.length,
      count,
      `${label}: expected ${count} watch callbacks before the quiet window`
    );
    const changed = await this.#waitForNextCallback(quietMs);
    assert.equal(
      changed,
      false,
      `${label}: expected no additional watch callbacks; observed ${this.#results.length}`
    );
  }

  async settle(quietMs: number, maxWaitMs: number, label: string): Promise<number> {
    const deadline = Date.now() + maxWaitMs;
    while (true) {
      const remainingMs = deadline - Date.now();
      if (remainingMs <= 0) {
        throw new Error(
          `${label}: watch callbacks did not settle within ${maxWaitMs}ms; observed ${this.#results.length}`
        );
      }

      const changed = await this.#waitForNextCallback(Math.min(quietMs, remainingMs));
      if (!changed) {
        return this.#results.length;
      }
    }
  }

  async #waitForNextCallback(timeoutMs: number): Promise<boolean> {
    return await new Promise<boolean>((resolve) => {
      let settled = false;
      const finish = (changed: boolean): void => {
        if (settled) {
          return;
        }
        settled = true;
        clearTimeout(timeout);
        this.#waiters = this.#waiters.filter((candidate) => candidate !== onCallback);
        resolve(changed);
      };
      const onCallback = (): void => finish(true);
      const timeout = setTimeout(() => finish(false), timeoutMs);
      this.#waiters.push(onCallback);
    });
  }
}

function startWebpackWatch(
  root: string,
  watchOptions: SharedWatchOptions
): WatchSession<WebpackStats> {
  const compiler = webpack(webpackNodeOptions(root)) as WebpackCompiler;
  const results = new WatchResults<WebpackStats>();
  const watching = compiler.watch(watchOptions, results.handler);
  assert.ok(watching, "webpack.watch returned a Watching handle");
  return {
    results,
    close: async () => {
      await closeWebpackWatching(watching);
      await closeWebpackCompiler(compiler);
    }
  };
}

function startUnpackWatch(
  root: string,
  watchOptions: SharedWatchOptions
): WatchSession<UnpackStats> {
  const compiler = unpack(unpackOptions(root)) as UnpackCompiler;
  const results = new WatchResults<UnpackStats>();
  const watching = compiler.watch(watchOptions, results.handler);
  return {
    results,
    close: async () => {
      await closeUnpackWatching(watching);
      await closeUnpackCompiler(compiler);
    }
  };
}

async function closeWebpackWatching(watching: WebpackWatching): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    watching.close((err) => {
      if (err) {
        reject(err);
      } else {
        resolve();
      }
    });
  });
}

async function closeUnpackWatching(watching: UnpackWatching): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    watching.close((err) => {
      if (err) {
        reject(err);
      } else {
        resolve();
      }
    });
  });
}

async function closeWatchSessions(
  ...sessions: Array<ClosableWatchSession | undefined>
): Promise<void> {
  const settled = await Promise.allSettled(
    sessions.map(async (session) => {
      await session?.close();
    })
  );
  const rejected = settled.find(
    (result): result is PromiseRejectedResult => result.status === "rejected"
  );
  if (rejected) {
    throw rejected.reason;
  }
}

async function settleWatchPair(
  webpackSession: WatchSession<WebpackStats>,
  unpackSession: WatchSession<UnpackStats>,
  label: string
): Promise<[number, number]> {
  return await Promise.all([
    webpackSession.results.settle(180, 1_500, `webpack ${label}`),
    unpackSession.results.settle(180, 1_500, `Unpack ${label}`)
  ]);
}

async function assertSuccessfulCalls<TWebpackStats extends StatsLike, TUnpackStats extends StatsLike>(
  webpackCalls: Array<Promise<WatchResult<TWebpackStats>>>,
  unpackCalls: Array<Promise<WatchResult<TUnpackStats>>>
): Promise<void> {
  const [webpackResults, unpackResults] = await Promise.all([
    Promise.all(webpackCalls),
    Promise.all(unpackCalls)
  ]);
  for (const result of webpackResults) {
    assertSuccessfulWatchResult(result, "webpack");
  }
  for (const result of unpackResults) {
    assertSuccessfulWatchResult(result, "Unpack");
  }
}

function assertSuccessfulWatchResult<TStats extends StatsLike>(
  result: WatchResult<TStats>,
  label: string
): void {
  assert.equal(result.err, null, `${label} watch callback err`);
  assert.ok(result.stats, `${label} watch callback stats`);
  assert.equal(result.stats.hasErrors(), false, `${label} stats.hasErrors()`);
}

async function assertAssetContains(root: string, asset: string, pattern: RegExp): Promise<void> {
  assert.match(await readAsset(root, asset), pattern);
}

let nextWriteMtime = Date.now() + 2_000;

async function writeSource(
  root: string,
  relativePath: string,
  source: string,
  options: { touchMtime?: boolean } = {}
): Promise<void> {
  const file = join(root, relativePath);
  await writeFile(file, source, { encoding: "utf8" });
  if (options.touchMtime === false) {
    return;
  }

  nextWriteMtime += 2_000;
  const changedTime = new Date(nextWriteMtime);
  await utimes(file, changedTime, changedTime);
}
