import { execFile as execFileCallback } from "node:child_process";
import { copyFile, mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { performance } from "node:perf_hooks";
import { dirname, join, resolve } from "node:path";
import { promisify } from "node:util";

import { DEFAULT_TURBOPACK_COMMIT } from "./runner.mjs";

const execFile = promisify(execFileCallback);
const require = createRequire(import.meta.url);
const preparedTurbopackBuilds = new Set();
const UNPACK_INTERNAL_TRACING_ENV = "UNPACK_INTERNAL_TRACING";
const DEFAULT_UNPACK_TRACING_FILTER = "unpack_core=trace,unpack_node=trace";
const TURBOPACK_TRACING_ENV = "TURBOPACK_TRACING";
const DEFAULT_TURBOPACK_TRACING_FILTER = "turbo-tasks";
const METRO_COMMONJS_TRANSFORMER = require.resolve("./metro-commonjs-transformer.cjs");

export const adapters = {
  unpack: {
    name: "unpack",
    versionSource: () => `@unpack-js/core@${packageVersion("@unpack-js/core")}`,
    async build({
      fixture,
      outputDir,
      cacheDir,
      phase,
      persistentCache = true,
      cacheReadonly = false,
      options
    }) {
      assertNoWebpackLoaderFixture(fixture, "Unpack");
      configureUnpackTracing({ fixture, phase, persistentCache, cacheReadonly, options });
      const { default: unpack } = await import("@unpack-js/core");
      const compiler = unpack({
        mode: "none",
        context: fixture.context,
        entry: fixture.entry,
        output: { path: outputDir },
        sourcemap: false,
        cache: persistentCache
          ? {
              type: "filesystem",
              cacheLocation: cacheDir,
              idleTimeout: 0,
              readonly: cacheReadonly
            }
          : false
      });

      try {
        const { err, stats } = await runUnpackCompiler(compiler);
        if (err) {
          throw err;
        }
        if (stats?.hasErrors()) {
          const errors = stats.toJson().errors.map((error) => error.message).join("\n");
          throw new Error(errors || "Unpack reported compilation errors");
        }
      } finally {
        await closeUnpackCompiler(compiler);
      }

      return { entryFile: join(outputDir, "main.js") };
    }
  },

  webpack: {
    name: "webpack",
    supportsWebpackLoaders: true,
    versionSource: () => `webpack@${packageVersion("webpack")}`,
    async build({
      fixture,
      outputDir,
      cacheDir,
      phase,
      persistentCache = true,
      cacheReadonly = false
    }) {
      const webpackModule = await import("webpack");
      const webpack = webpackModule.default ?? webpackModule;
      const tracing = createWebpackPhaseTracing({
        fixture,
        phase,
        persistentCache,
        cacheReadonly
      });
      const compiler = webpack({
        ...webpackLikeConfig({ fixture, outputDir }),
        cache: persistentCache
          ? {
              type: "filesystem",
              cacheDirectory: cacheDir,
              readonly: cacheReadonly
            }
          : false,
        plugins: [tracing.plugin]
      });

      try {
        const stats = await runWebpackCompiler(compiler);
        assertWebpackStats(stats, "webpack");
      } finally {
        await tracing.close(compiler);
      }

      return { entryFile: join(outputDir, "main.js") };
    }
  },

  rspack: {
    name: "rspack",
    supportsWebpackLoaders: true,
    versionSource: () => `@rspack/core@${packageVersion("@rspack/core")}`,
    async build({ fixture, outputDir, cacheDir, persistentCache = true }) {
      const rspackModule = await import("@rspack/core");
      const rspack = rspackModule.rspack ?? rspackModule.default;
      const compiler = rspack({
        ...webpackLikeConfig({ fixture, outputDir }),
        cache: persistentCache
          ? {
              type: "persistent",
              storage: {
                type: "filesystem",
                directory: cacheDir
              }
            }
          : false
      });

      try {
        const stats = await runWebpackCompiler(compiler);
        assertWebpackStats(stats, "Rspack");
      } finally {
        await closeWebpackCompiler(compiler);
      }

      return { entryFile: join(outputDir, "main.js") };
    }
  },

  rolldown: {
    name: "rolldown",
    versionSource: () => `rolldown@${packageVersion("rolldown")}`,
    async build({ fixture, outputDir }) {
      assertNoWebpackLoaderFixture(fixture, "Rolldown");
      const { rolldown } = await import("rolldown");
      const bundle = await rolldown({
        input: resolve(fixture.context, fixture.entry),
        logLevel: "silent",
        treeshake: false
      });

      try {
        await bundle.write({
          dir: outputDir,
          format: "cjs",
          entryFileNames: "main.js",
          chunkFileNames: "[name].js",
          exports: "named",
          sourcemap: false
        });
      } finally {
        await bundle.close?.();
      }

      return { entryFile: join(outputDir, "main.js") };
    }
  },

  metro: {
    name: "metro",
    versionSource: () => `metro@${packageVersion("metro")}`,
    async build({ fixture, outputDir, cacheDir, persistentCache = true }) {
      assertNoWebpackLoaderFixture(fixture, "Metro");
      const metroModule = await import("metro");
      const metroRuntimeRoot = resolveMetroRuntimeRoot();
      const transformCacheDir = join(cacheDir, "transform");
      const fileMapCacheDir = join(cacheDir, "file-map");
      const entryFile = join(outputDir, "main.js");

      await mkdir(transformCacheDir, { recursive: true });
      await mkdir(fileMapCacheDir, { recursive: true });

      const config = await metroModule.loadConfig(
        { cwd: fixture.context, verbose: false },
        {
          projectRoot: fixture.context,
          watchFolders: [metroRuntimeRoot],
          cacheStores: persistentCache
            ? (MetroCache) => [
                new MetroCache.FileStore({ root: transformCacheDir })
              ]
            : [],
          resetCache: !persistentCache,
          maxWorkers: 1,
          stickyWorkers: false,
          resolver: {
            useWatchman: false,
            extraNodeModules: {
              "metro-runtime": metroRuntimeRoot
            }
          },
          serializer: {
            getRunModuleStatement: (moduleId) =>
              `module.exports = __r(${JSON.stringify(moduleId)});`
          },
          transformer: {
            babelTransformerPath: METRO_COMMONJS_TRANSFORMER,
            enableBabelRCLookup: false,
            enableBabelRuntime: false
          }
        }
      );

      config.fileMapCacheDirectory = fileMapCacheDir;

      await metroModule.runBuild(config, {
        entry: fixture.entry,
        out: entryFile,
        dev: false,
        minify: false,
        sourceMap: false,
        platform: "web"
      });

      return { entryFile };
    }
  },

  parcel: {
    name: "parcel",
    versionSource: () => `parcel@${packageVersion("parcel")}`,
    async build({
      fixture,
      outputDir,
      cacheDir,
      persistentCache = true,
      cacheReadonly = false
    }) {
      assertNoWebpackLoaderFixture(fixture, "Parcel");
      const parcelRequire = createParcelRequire();
      const { default: Parcel } = parcelRequire("@parcel/core");
      const currentCwd = process.cwd();
      const currentExecArgv = process.execArgv;
      const entryFile = join(outputDir, "main.js");

      try {
        await rm(entryFile, { force: true });
        process.chdir(fixture.context);
        // Parcel forwards process.execArgv to worker threads. Node's test runner can
        // include flags that Worker rejects, so keep the benchmark invocation clean.
        process.execArgv = [];
        const bundler = new Parcel({
          entries: [fixture.entry],
          projectRoot: fixture.context,
          defaultConfig: parcelRequire.resolve("@parcel/config-default"),
          mode: "production",
          shouldPatchConsole: false,
          shouldDisableCache: !persistentCache || cacheReadonly,
          shouldAutoInstall: false,
          shouldContentHash: false,
          cacheDir,
          logLevel: "none",
          defaultTargetOptions: {
            shouldOptimize: false,
            shouldScopeHoist: true,
            sourceMaps: false,
            distDir: outputDir
          },
          targets: {
            main: {
              distDir: outputDir,
              distEntry: "main.js",
              context: "node",
              outputFormat: "commonjs",
              isLibrary: true,
              includeNodeModules: true,
              optimize: false,
              scopeHoist: true,
              sourceMap: false,
              engines: {
                node: ">=16"
              }
            }
          }
        });

        await bundler.run();
      } finally {
        process.execArgv = currentExecArgv;
        process.chdir(currentCwd);
      }

      return { entryFile };
    }
  },

  turbopack: {
    name: "turbopack",
    supportsLoaderFixture: true,
    outputDir: ({ fixture }) => join(fixture.context, "dist"),
    versionSource: ({ options }) => {
      if (options.turbopackBinary) {
        return `hardfist/bundler-diff@${options.turbopackCommit ?? "turbopack-cli-main"}+release-turbopack-cli`;
      }
      return `vercel/next.js@${options.turbopackCommit ?? DEFAULT_TURBOPACK_COMMIT}+benchmark-cache-flush`;
    },
    async prepare({ options }) {
      if (options.turbopackBinary) {
        return;
      }

      const repo = options.turbopackRepo;
      if (!repo) {
        throw unsupported(
          "Turbopack requires --turbopack-binary or --turbopack-repo pointing at a fixed Next.js checkout"
        );
      }

      const profile = options.turbopackProfile ?? "release";
      const prepareKey = `${repo}:${profile}`;
      if (preparedTurbopackBuilds.has(prepareKey)) {
        return;
      }

      await applyTurbopackBuildCacheFlushPatch(repo);

      const args = ["build", "--package", "turbopack-cli", "--bin", "turbopack-cli"];
      if (profile === "release") {
        args.push("--release");
      } else if (profile !== "dev") {
        args.push("--profile", profile);
      }

      await execFile("cargo", args, {
        cwd: repo,
        timeout: 60 * 60 * 1000,
        maxBuffer: 1024 * 1024 * 20
      });
      preparedTurbopackBuilds.add(prepareKey);
    },
    async build({ fixture, cacheDir, phase, persistentCache = true, options }) {
      const repo = options.turbopackRepo;
      const profile = options.turbopackProfile ?? "release";
      const binary = options.turbopackBinary
        ? options.turbopackBinary
        : repo
          ? join(repo, "target", profile === "dev" ? "debug" : profile, "turbopack-cli")
          : null;
      if (!binary) {
        throw unsupported(
          "Turbopack requires --turbopack-binary or --turbopack-repo pointing at a fixed Next.js checkout"
        );
      }
      await materializeTurbopackLoaderFixture(fixture);

      const args = [
        "build",
        "--dir",
        fixture.context,
        "--root",
        fixture.context,
        "--target",
        "node",
        "--no-minify",
        "--no-sourcemap",
        "--no-scope-hoist"
      ];
      if (persistentCache) {
        args.push("--persistent-caching", "--cache-dir", cacheDir);
      }
      args.push(fixture.entry);

      const tracingFilter = turbopackTracingFilter(options);
      const traceSourcePath = join(fixture.context, ".turbopack", "trace.log");
      if (tracingFilter) {
        await rm(traceSourcePath, { force: true });
      }

      const env = { ...process.env, CI: process.env.CI ?? "1" };
      delete env[TURBOPACK_TRACING_ENV];
      if (tracingFilter) {
        env[TURBOPACK_TRACING_ENV] = tracingFilter;
      }

      try {
        await execFile(
          binary,
          args,
          {
            cwd: repo ?? dirname(binary),
            env,
            timeout: 10 * 60 * 1000,
            maxBuffer: 1024 * 1024 * 20
          }
        );
      } finally {
        if (tracingFilter && options.turbopackTracingDir) {
          await archiveTurbopackTrace({
            sourcePath: traceSourcePath,
            targetPath: join(
              options.turbopackTracingDir,
              pathSegment(fixture.name ?? "fixture"),
              pathSegment(phase ?? "build"),
              "trace.log"
            ),
            fixture,
            phase,
            filter: tracingFilter
          });
        }
      }

      return { entryFile: join(fixture.context, "dist/index.entry.js") };
    }
  }
};

export async function applyTurbopackBuildCacheFlushPatch(repo) {
  const buildSourcePath = join(
    repo,
    "turbopack",
    "crates",
    "turbopack-cli",
    "src",
    "build",
    "mod.rs"
  );
  const source = await readFile(buildSourcePath, "utf8");

  if (source.includes("tt.stop_and_wait().await;")) {
    return;
  }

  const target = `    builder.build().await?;

    // Intentionally leak this \`Arc\`. Otherwise we'll waste time during process exit performing a
`;
  const replacement = `    builder.build().await?;

    if args.common.persistent_caching {
        // Benchmark patch: flush ReadWriteOnShutdown storage before process exit.
        tt.stop_and_wait().await;
    }

    // Intentionally leak this \`Arc\`. Otherwise we'll waste time during process exit performing a
`;

  if (!source.includes(target)) {
    throw new Error("unable to patch Turbopack build cache shutdown path");
  }

  await writeFile(buildSourcePath, source.replace(target, replacement), "utf8");
}

function webpackLikeConfig({ fixture, outputDir }) {
  const config = {
    mode: "none",
    target: "node",
    context: fixture.context,
    entry: {
      main: resolve(fixture.context, fixture.entry)
    },
    output: {
      path: outputDir,
      filename: "main.js",
      chunkFilename: "[name].js",
      library: {
        type: "commonjs2"
      },
      clean: false
    },
    devtool: false,
    optimization: {
      minimize: false
    },
    stats: "errors-warnings"
  };

  const rules = webpackLoaderRules(fixture);
  if (rules.length > 0) {
    config.module = { rules };
  }

  return config;
}

function webpackLoaderRules(fixture) {
  if (!fixture.requiresWebpackLoaders) {
    return [];
  }

  return [
    {
      test: /\.benchdata$/,
      loader: join(fixture.context, "loaders/benchmark-loader.cjs")
    }
  ];
}

function assertNoWebpackLoaderFixture(fixture, bundler) {
  if (!fixture.requiresWebpackLoaders) {
    return;
  }

  throw unsupported(`${bundler} does not support the loader benchmark fixture`);
}

async function materializeTurbopackLoaderFixture(fixture) {
  if (!fixture.requiresWebpackLoaders) {
    return;
  }

  const entryPath = resolve(fixture.context, fixture.entry);
  const entrySource = await readFile(entryPath, "utf8");
  const rewrittenEntrySource = entrySource.replace(
    /(\.\/loader-data\/item\d+\.benchdata)(?=")/g,
    "$1.js"
  );
  if (rewrittenEntrySource !== entrySource) {
    await writeFile(entryPath, rewrittenEntrySource, "utf8");
  }

  for (let index = 0; index < fixture.loaderModuleCount; index += 1) {
    const dataPath = join(fixture.context, `src/loader-data/item${index}.benchdata`);
    const value = Number.parseInt((await readFile(dataPath, "utf8")).trim(), 10);
    if (!Number.isFinite(value)) {
      throw new Error(`benchmark loader expected a numeric payload in ${dataPath}`);
    }
    await writeFile(
      `${dataPath}.js`,
      [
        `const value = ${value};`,
        "export default value;",
        "export { value as loadedValue };"
      ].join("\n") + "\n",
      "utf8"
    );
  }
}

function runUnpackCompiler(compiler) {
  return new Promise((resolve) => {
    compiler.run((err, stats) => resolve({ err, stats }));
  });
}

function closeUnpackCompiler(compiler) {
  return new Promise((resolve, reject) => {
    compiler.close((err) => {
      if (err) {
        reject(err);
        return;
      }
      resolve();
    });
  });
}

function runWebpackCompiler(compiler) {
  return new Promise((resolve, reject) => {
    compiler.run((err, stats) => {
      if (err) {
        reject(err);
        return;
      }
      resolve(stats);
    });
  });
}

function closeWebpackCompiler(compiler) {
  return new Promise((resolve, reject) => {
    compiler.close((err) => {
      if (err) {
        reject(err);
        return;
      }
      resolve();
    });
  });
}

function assertWebpackStats(stats, label) {
  if (!stats?.hasErrors?.()) {
    return;
  }

  const info = stats.toJson({
    all: false,
    errors: true
  });
  const message =
    info.errors?.map((error) => error.message || String(error)).join("\n") ||
    `${label} reported compilation errors`;
  throw new Error(message);
}

function packageVersion(packageName) {
  try {
    return require(`${packageName}/package.json`).version;
  } catch {
    return packageVersionFromResolvedEntry(packageName);
  }
}

function packageVersionFromResolvedEntry(packageName) {
  let current = dirname(require.resolve(packageName));
  while (current !== dirname(current)) {
    const packageJson = join(current, "package.json");
    try {
      const parsed = JSON.parse(require("node:fs").readFileSync(packageJson, "utf8"));
      if (parsed.name === packageName) {
        return parsed.version;
      }
    } catch {
      // Keep walking upward until the package root is found.
    }
    current = dirname(current);
  }
  return "unknown";
}

function resolveMetroRuntimeRoot() {
  const metroRoot = dirname(require.resolve("metro/package.json"));
  return dirname(
    require.resolve("metro-runtime/package.json", { paths: [metroRoot] })
  );
}

function createParcelRequire() {
  const parcelRoot = dirname(require.resolve("parcel/package.json"));
  return createRequire(`${parcelRoot}/package.json`);
}

function configureUnpackTracing({ fixture, phase, persistentCache, cacheReadonly, options }) {
  const filter = unpackTracingFilter(options);
  if (!filter) {
    process.env[UNPACK_INTERNAL_TRACING_ENV] = "off";
    return;
  }

  process.env[UNPACK_INTERNAL_TRACING_ENV] = filter;
  process.stderr.write(
    [
      "[unpack tracing]",
      `fixture=${fixture.name}`,
      `phase=${phase}`,
      `persistent_cache=${persistentCache ? "on" : "off"}`,
      `cache_readonly=${cacheReadonly ? "on" : "off"}`,
      `filter=${filter}`
    ].join(" ") + "\n"
  );
}

function createWebpackPhaseTracing({ fixture, phase, persistentCache, cacheReadonly }) {
  const pluginName = "UnpackBenchmarkWebpackPhaseTracingPlugin";
  const started = new Map();
  const durations = new Map();

  function start(name) {
    if (!started.has(name)) {
      started.set(name, performance.now());
    }
  }

  function end(name) {
    const startTime = started.get(name);
    if (startTime === undefined) {
      return;
    }
    started.delete(name);
    durations.set(name, (durations.get(name) ?? 0) + performance.now() - startTime);
  }

  function print() {
    process.stderr.write(
      [
        "[webpack tracing]",
        `fixture=${fixture.name}`,
        `phase=${phase}`,
        `persistent_cache=${persistentCache ? "on" : "off"}`,
        `cache_readonly=${cacheReadonly ? "on" : "off"}`
      ].join(" ") + "\n"
    );

    for (const [span, duration] of [
      ["Webpack::run", durations.get("compilerRun")],
      ["Webpack::make", durations.get("make")],
      ["Webpack::build_chunk_graph", durations.get("chunkGraph")],
      ["Webpack::create_assets", durations.get("createAssets")],
      ["Webpack::emit_assets", durations.get("emitAssets")],
      ["Webpack::flush_cache", durations.get("flushCache")]
    ]) {
      if (duration === undefined) {
        continue;
      }
      process.stderr.write(
        `TRACE ${span}: webpack: close time.busy=${formatDurationMs(duration)} time.idle=0ms\n`
      );
    }
  }

  return {
    plugin: {
      apply(compiler) {
        compiler.hooks.beforeRun.tap(pluginName, () => start("compilerRun"));
        compiler.hooks.run.tap(pluginName, () => start("compilerRun"));
        compiler.hooks.done.tap(pluginName, () => end("compilerRun"));
        compiler.hooks.make.tap(pluginName, () => start("make"));
        compiler.hooks.finishMake.tap(pluginName, () => end("make"));
        compiler.hooks.emit.tap(pluginName, () => start("emitAssets"));
        compiler.hooks.afterEmit.tap(pluginName, () => end("emitAssets"));
        compiler.hooks.compilation.tap(pluginName, (compilation) => {
          compilation.hooks.beforeChunks.tap(pluginName, () => start("chunkGraph"));
          compilation.hooks.afterChunks.tap(pluginName, () => end("chunkGraph"));
          compilation.hooks.beforeCodeGeneration.tap(pluginName, () => start("createAssets"));
          compilation.hooks.afterProcessAssets.tap(pluginName, () => end("createAssets"));
        });
      }
    },
    async close(compiler) {
      const startedClose = performance.now();
      try {
        await closeWebpackCompiler(compiler);
      } finally {
        durations.set("flushCache", performance.now() - startedClose);
        print();
      }
    }
  };
}

function formatDurationMs(duration) {
  return `${duration.toFixed(3)}ms`;
}

function unpackTracingFilter(options) {
  const value = options?.unpackTracing ?? DEFAULT_UNPACK_TRACING_FILTER;
  if (value === false) {
    return null;
  }
  const normalized = String(value).trim();
  const lowered = normalized.toLowerCase();
  if (
    normalized === "" ||
    normalized === "0" ||
    lowered === "false" ||
    lowered === "off" ||
    lowered === "none"
  ) {
    return null;
  }
  if (normalized === "1" || lowered === "true" || lowered === "on") {
    return DEFAULT_UNPACK_TRACING_FILTER;
  }
  return normalized;
}

function turbopackTracingFilter(options) {
  if (!Object.hasOwn(options ?? {}, "turbopackTracing")) {
    return null;
  }
  const value = options.turbopackTracing;
  if (value === false) {
    return null;
  }
  const normalized = String(value ?? "").trim();
  const lowered = normalized.toLowerCase();
  if (
    normalized === "" ||
    normalized === "0" ||
    lowered === "false" ||
    lowered === "off" ||
    lowered === "none"
  ) {
    return null;
  }
  if (normalized === "1" || lowered === "true" || lowered === "on") {
    return DEFAULT_TURBOPACK_TRACING_FILTER;
  }
  return normalized;
}

async function archiveTurbopackTrace({
  sourcePath,
  targetPath,
  fixture,
  phase,
  filter
}) {
  let traceStat;
  try {
    traceStat = await stat(sourcePath);
  } catch (error) {
    if (error?.code !== "ENOENT") {
      process.stderr.write(
        `[turbopack tracing] unable to inspect ${sourcePath}: ${errorMessage(error)}\n`
      );
    }
    return;
  }

  if (!traceStat.isFile() || traceStat.size === 0) {
    return;
  }

  try {
    await mkdir(dirname(targetPath), { recursive: true });
    await copyFile(sourcePath, targetPath);
    await writeFile(
      join(dirname(targetPath), "metadata.json"),
      `${JSON.stringify(
        {
          fixture: fixture.name ?? null,
          phase: phase ?? null,
          filter,
          source: sourcePath,
          bytes: traceStat.size
        },
        null,
        2
      )}\n`,
      "utf8"
    );
    process.stderr.write(
      [
        "[turbopack tracing]",
        `fixture=${fixture.name ?? "unknown"}`,
        `phase=${phase ?? "unknown"}`,
        `filter=${filter}`,
        `file=${targetPath}`,
        `bytes=${traceStat.size}`
      ].join(" ") + "\n"
    );
  } catch (error) {
    process.stderr.write(
      `[turbopack tracing] unable to archive ${sourcePath}: ${errorMessage(error)}\n`
    );
  }
}

function pathSegment(value) {
  const segment = String(value).replace(/[^A-Za-z0-9._-]+/g, "-");
  return segment || "unknown";
}

function unsupported(message) {
  const error = new Error(message);
  error.code = "UNSUPPORTED_BUNDLER";
  return error;
}
