import { createRequire } from "node:module";
import { mkdir, readdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import { dirname, join, resolve, sep } from "node:path";

import {
  applyWarmBuildMutation,
  createBenchmarkFixture,
  FIXTURE_SHAPES
} from "./fixture.mjs";

const require = createRequire(import.meta.url);

export const DEFAULT_BUNDLERS = [
  "unpack",
  "webpack",
  "rspack",
  "rolldown",
  "metro",
  "parcel",
  "turbopack"
];

export const DEFAULT_FIXTURES = ["large", "loader"];

export const DEFAULT_TURBOPACK_COMMIT =
  "a88f25caf0070b582a8ed83b1ae9e7135d7fd3bc";

export async function runBenchmark(options = {}) {
  const workspaceDir = resolve(options.workspaceDir ?? ".benchmark-work");
  const fixtureNames = options.fixtures ?? DEFAULT_FIXTURES;
  const bundlerNames = options.bundlers ?? DEFAULT_BUNDLERS;
  const adapters = options.adapters ?? (await defaultAdapters());
  const shapes = fixtureNames.map((name) => {
    const shape = FIXTURE_SHAPES[name];
    if (!shape) {
      throw new Error(`unknown benchmark fixture '${name}'`);
    }
    return shape;
  });

  await mkdir(workspaceDir, { recursive: true });
  const fixtureRoot = join(workspaceDir, "fixtures");
  const results = [];

  for (const shape of shapes) {
    for (const bundler of bundlerNames) {
      const fixture = await createBenchmarkFixture(fixtureRoot, shape);
      const adapter = adapters[bundler];
      results.push(
        await runBundlerBenchmark({
          adapter,
          bundler,
          fixture,
          workspaceDir,
          options
        })
      );
    }
  }

  return {
    schema_version: 2,
    generated_at: new Date().toISOString(),
    results
  };
}

export function toSummaryMarkdown(report, baselineReport) {
  const baselines = new Map(
    (baselineReport?.results ?? []).map((result) => [resultKey(result), result])
  );
  const groups = [
    ["### Loader Benchmarks", report.results.filter((result) => result.fixture === "loader")],
    [
      "### Benchmarks Without Loaders",
      report.results.filter((result) => result.fixture !== "loader")
    ]
  ].filter(([, results]) => results.length > 0);

  const summary = groups
    .map(([heading, results]) => `${heading}\n\n${toSummaryTable(results, baselines)}`)
    .join("\n\n");
  const comparisonNote = baselineReport
    ? "\n\n> Delta vs main: `+` means slower or larger; `−` means faster or smaller. Calculated as `(current - main) / main`."
    : "";

  return `${summary}${comparisonNote}\n`;
}

function toSummaryTable(results, baselines) {
  const lines = [
    "| fixture | bundler | version/source | cold_build_ms | warm_build_ms | no_cache_build_ms | output_bytes | status |",
    "| --- | --- | --- | ---: | ---: | ---: | ---: | --- |"
  ];

  for (const result of results) {
    const baseline = baselines.get(resultKey(result));
    lines.push(
      [
        result.fixture,
        result.bundler,
        result.version_source ?? "",
        formatMeasurement(result.cold_build_ms, baseline?.cold_build_ms),
        formatMeasurement(result.warm_build_ms, baseline?.warm_build_ms),
        formatMeasurement(result.no_cache_build_ms, baseline?.no_cache_build_ms),
        formatMeasurement(result.output_bytes, baseline?.output_bytes, 0),
        result.status
      ].join(" | ").replace(/^/, "| ").replace(/$/, " |")
    );
  }

  return lines.join("\n");
}

function resultKey(result) {
  return `${result.fixture}\0${result.bundler}`;
}

function formatMeasurement(current, baseline, digits = 1) {
  const value = formatNumber(current, digits);
  if (!Number.isFinite(current) || !Number.isFinite(baseline) || baseline === 0) {
    return value;
  }
  const percentage = ((current - baseline) / baseline) * 100;
  const delta = `${percentage >= 0 ? "+" : ""}${percentage.toFixed(1)}%`;
  return `${value} (${delta})`;
}

async function runBundlerBenchmark({ adapter, bundler, fixture, workspaceDir, options }) {
  const versionSource = await adapterVersion(adapter, options);
  const baseDir = join(workspaceDir, "runs", fixture.name, bundler);
  const outputDir = adapter?.outputDir
    ? adapter.outputDir({ fixture, baseDir, options })
    : join(baseDir, "output");
  const cacheDir = join(baseDir, "cache");
  const noCacheBaseDir = join(baseDir, "no-cache");
  const noCacheOutputDir = adapter?.outputDir
    ? adapter.outputDir({ fixture, baseDir: noCacheBaseDir, options })
    : join(noCacheBaseDir, "output");
  const noCacheCacheDir = join(noCacheBaseDir, "cache");

  if (!adapter) {
    return emptyResult({
      fixture,
      bundler,
      versionSource: "not_configured",
      status: "unsupported",
      message: "no adapter configured"
    });
  }

  if (
    fixture.requiresWebpackLoaders &&
    !adapter.supportsWebpackLoaders &&
    !adapter.supportsLoaderFixture
  ) {
    return emptyResult({
      fixture,
      bundler,
      versionSource,
      status: "unsupported",
      message: `${adapter.name ?? bundler} does not support the loader benchmark fixture`
    });
  }

  try {
    await adapter.prepare?.({ options });
  } catch (error) {
    return emptyResult({
      fixture,
      bundler,
      versionSource,
      status: isUnsupported(error) ? "unsupported" : "setup_failed",
      message: errorMessage(error)
    });
  }

  const cold = await timedBuild({
    adapter,
    phase: "cold",
    fixture,
    outputDir,
    cacheDir,
    persistentCache: true,
    cacheReadonly: false,
    options
  });
  if (cold.status !== "success") {
    return resultFromPhases({ fixture, bundler, versionSource, cold });
  }

  await applyWarmBuildMutation(fixture);

  const warm = await timedBuild({
    adapter,
    phase: "warm",
    fixture,
    outputDir,
    cacheDir,
    persistentCache: true,
    cacheReadonly: true,
    options
  });

  if (warm.status !== "success") {
    return resultFromPhases({ fixture, bundler, versionSource, cold, warm });
  }

  const noCache = await timedBuild({
    adapter,
    phase: "no-cache",
    fixture,
    outputDir: noCacheOutputDir,
    cacheDir: noCacheCacheDir,
    persistentCache: false,
    cacheReadonly: false,
    options
  });

  return resultFromPhases({ fixture, bundler, versionSource, cold, warm, noCache });
}

async function timedBuild({
  adapter,
  phase,
  fixture,
  outputDir,
  cacheDir,
  persistentCache,
  cacheReadonly = false,
  options
}) {
  if (phase === "cold" || phase === "no-cache") {
    await rm(outputDir, { recursive: true, force: true });
    await rm(cacheDir, { recursive: true, force: true });
  }
  await mkdir(outputDir, { recursive: true });
  await mkdir(cacheDir, { recursive: true });

  let buildResult;
  const started = performance.now();
  try {
    buildResult = await adapter.build({
      fixture,
      outputDir,
      cacheDir,
      phase,
      persistentCache,
      cacheReadonly,
      options
    });
  } catch (error) {
    return {
      status: isUnsupported(error) ? "unsupported" : "build_failed",
      build_ms: elapsed(started),
      output_bytes: null,
      verify_ms: null,
      message: errorMessage(error)
    };
  }

  const buildMs = elapsed(started);
  const entryFile = buildResult?.entryFile;
  if (!entryFile) {
    return {
      status: "build_failed",
      build_ms: buildMs,
      output_bytes: await outputBytes(outputDir),
      verify_ms: null,
      message: "adapter did not return an entry file"
    };
  }

  const verifyStarted = performance.now();
  try {
    await verifyBundle({
      entryFile,
      outputDir,
      expectedChecksum: fixture.expectedChecksum
    });
  } catch (error) {
    return {
      status: "runtime_failed",
      build_ms: buildMs,
      output_bytes: await outputBytes(outputDir),
      verify_ms: elapsed(verifyStarted),
      message: errorMessage(error)
    };
  }

  return {
    status: "success",
    build_ms: buildMs,
    output_bytes: await outputBytes(outputDir),
    verify_ms: elapsed(verifyStarted),
    message: null
  };
}

async function verifyBundle({ entryFile, outputDir, expectedChecksum }) {
  await writeFile(
    join(outputDir, "package.json"),
    `${JSON.stringify({ type: "commonjs" }, null, 2)}\n`,
    "utf8"
  );

  clearRequireCache(outputDir);
  const resolvedEntry = require.resolve(entryFile);
  delete require.cache[resolvedEntry];
  const previousDocument = globalThis.document;
  const hadDocument = Object.hasOwn(globalThis, "document");
  const previousConsoleLog = console.log;
  globalThis.document = {
    getElementById() {
      return null;
    }
  };
  console.log = () => {};
  let exports;
  try {
    exports = require(resolvedEntry);
  } finally {
    console.log = previousConsoleLog;
    if (hadDocument) {
      globalThis.document = previousDocument;
    } else {
      delete globalThis.document;
    }
  }
  const checksum = exports?.checksum ?? exports?.default?.checksum;

  if (checksum !== expectedChecksum) {
    throw new Error(
      `expected bundle checksum ${expectedChecksum}, received ${String(checksum)}`
    );
  }
}

function resultFromPhases({ fixture, bundler, versionSource, cold, warm, noCache }) {
  const status =
    cold.status !== "success"
      ? cold.status
      : warm && warm.status !== "success"
        ? `warm_${warm.status}`
        : noCache && noCache.status !== "success"
          ? `no_cache_${noCache.status}`
        : "success";
  const message =
    cold.status !== "success"
      ? cold.message
      : warm?.status !== "success"
        ? warm.message
        : noCache?.status !== "success"
          ? noCache.message
          : null;

  return {
    fixture: fixture.name,
    bundler,
    version_source: versionSource,
    cold_build_ms: cold.status === "success" ? cold.build_ms : null,
    warm_build_ms: warm?.status === "success" ? warm.build_ms : null,
    no_cache_build_ms: noCache?.status === "success" ? noCache.build_ms : null,
    output_bytes: warm?.output_bytes ?? noCache?.output_bytes ?? cold.output_bytes,
    cold_status: cold.status,
    warm_status: warm?.status ?? "not_run",
    no_cache_status: noCache?.status ?? "not_run",
    verify_status:
      cold.status === "runtime_failed" ||
      warm?.status === "runtime_failed" ||
      noCache?.status === "runtime_failed"
        ? "runtime_failed"
        : cold.status === "success" &&
            (!warm || warm.status === "success") &&
            (!noCache || noCache.status === "success")
          ? "success"
          : "not_run",
    status,
    error: message
  };
}

function emptyResult({ fixture, bundler, versionSource, status, message }) {
  return {
    fixture: fixture.name,
    bundler,
    version_source: versionSource,
    cold_build_ms: null,
    warm_build_ms: null,
    no_cache_build_ms: null,
    output_bytes: null,
    cold_status: "not_run",
    warm_status: "not_run",
    no_cache_status: "not_run",
    verify_status: "not_run",
    status,
    error: message
  };
}

async function adapterVersion(adapter, options) {
  if (!adapter) {
    return "not_configured";
  }
  if (adapter.versionSource) {
    return adapter.versionSource({ options });
  }
  return adapter.name;
}

export async function outputBytes(outputDir) {
  let total = 0;
  let entries;
  try {
    entries = await readdir(outputDir, { withFileTypes: true });
  } catch {
    return null;
  }

  for (const entry of entries) {
    if (entry.name === "package.json") {
      continue;
    }
    const path = join(outputDir, entry.name);
    if (entry.isDirectory()) {
      total += (await outputBytes(path)) ?? 0;
    } else if (entry.isFile()) {
      total += (await stat(path)).size;
    }
  }
  return total;
}

function clearRequireCache(outputDir) {
  const normalizedOutputDir = `${resolve(outputDir)}${sep}`;
  for (const cacheKey of Object.keys(require.cache)) {
    if (cacheKey === resolve(outputDir) || cacheKey.startsWith(normalizedOutputDir)) {
      delete require.cache[cacheKey];
    }
  }
}

function elapsed(started) {
  return Number((performance.now() - started).toFixed(3));
}

function formatNumber(value, digits = 3) {
  if (value === null || value === undefined) {
    return "";
  }
  return Number(value).toFixed(digits);
}

function isUnsupported(error) {
  return error && typeof error === "object" && error.code === "UNSUPPORTED_BUNDLER";
}

function errorMessage(error) {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

async function defaultAdapters() {
  const { adapters } = await import("./adapters.mjs");
  return adapters;
}
