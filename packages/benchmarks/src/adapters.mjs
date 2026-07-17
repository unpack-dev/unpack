import { execFile as execFileCallback } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFile, cp, mkdir, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { performance } from "node:perf_hooks";
import { dirname, join, relative, resolve } from "node:path";
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
const SWC_LOADER = require.resolve("swc-loader");
const UNPACK_PACKAGE_ROOT = resolve(dirname(require.resolve("@unpack-js/core")), "..");
const RSPACK_PACKAGE_ROOT = resolve(dirname(require.resolve("@rspack/core")), "..");

export const adapters = {
  unpack: {
    name: "unpack",
    supportsWebpackLoaders: true,
    versionSource: () => `@unpack-js/core@${packageVersion("@unpack-js/core")}`,
    async watchBuild({ fixture, outputDir, mutateAfterInitialBuild, options }) {
      configureUnpackTracing({
        fixture,
        phase: "watch",
        persistentCache: false,
        cacheReadonly: false,
        options
      });
      const { default: unpack } = await import("@unpack-js/core");
      const rules = webpackLoaderRules(fixture);
      const compiler = unpack({
        mode: "development",
        context: fixture.context,
        entry: fixture.entry,
        output: { path: outputDir },
        sourcemap: false,
        ...(rules.length === 0 ? {} : { module: { rules } }),
        cache: true
      });
      const rebuildMs = await runCompilerWatchBuild({
        compiler,
        mutateAfterInitialBuild,
        label: "Unpack",
        validate: ({ err, stats }) => {
          if (err) throw err;
          if (stats?.hasErrors()) {
            throw new Error(stats.toJson().errors.map((error) => error.message).join("\n"));
          }
        }
      });
      return { entryFile: join(outputDir, "main.js"), rebuildMs };
    },
    async build({
      fixture,
      outputDir,
      cacheDir,
      phase,
      persistentCache = true,
      cacheReadonly = false,
      options
    }) {
      configureUnpackTracing({ fixture, phase, persistentCache, cacheReadonly, options });
      const { default: unpack } = await import("@unpack-js/core");
      const rules = webpackLoaderRules(fixture);
      const compiler = unpack({
        mode: "none",
        context: fixture.context,
        entry: fixture.entry,
        output: { path: outputDir },
        sourcemap: false,
        snapshot: {
          managedPaths: [UNPACK_PACKAGE_ROOT]
        },
        ...(rules.length === 0 ? {} : { module: { rules } }),
        cache: persistentCache
          ? {
              type: "filesystem",
              cacheLocation: cacheDir,
              idleTimeout: 0,
              readonly: cacheReadonly
            }
          : false
      });

      let moduleCount;
      try {
        const { err, stats } = await runUnpackCompiler(compiler);
        if (err) {
          throw err;
        }
        if (stats?.hasErrors()) {
          const errors = stats.toJson().errors.map((error) => error.message).join("\n");
          throw new Error(errors || "Unpack reported compilation errors");
        }
        moduleCount = stats?.compilation.modules.size;
      } finally {
        await closeUnpackCompiler(compiler);
      }

      return { entryFile: join(outputDir, "main.js"), moduleCount };
    }
  },

  webpack: {
    name: "webpack",
    supportsWebpackLoaders: true,
    versionSource: () => `webpack@${packageVersion("webpack")}`,
    async watchBuild({ fixture, outputDir, mutateAfterInitialBuild }) {
      const webpackModule = await import("webpack");
      const webpack = webpackModule.default ?? webpackModule;
      const tracing = createWebpackLikePhaseTracing({
        bundler: "webpack",
        spanPrefix: "Webpack",
        fixture,
        phase: "watch",
        persistentCache: false,
        cacheReadonly: false
      });
      const compiler = webpack({
        ...webpackLikeConfig({ fixture, outputDir, mode: "development" }),
        cache: true,
        plugins: [tracing.plugin]
      });
      const rebuildMs = await runCompilerWatchBuild({
        compiler,
        closeCompiler: () => tracing.close(compiler),
        mutateAfterInitialBuild,
        label: "webpack",
        validate: ({ err, stats }) => {
          if (err) throw err;
          assertWebpackStats(stats, "webpack");
        }
      });
      return { entryFile: join(outputDir, "main.js"), rebuildMs };
    },
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
      const tracing = createWebpackLikePhaseTracing({
        bundler: "webpack",
        spanPrefix: "Webpack",
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

      let moduleCount;
      try {
        const stats = await runWebpackCompiler(compiler);
        assertWebpackStats(stats, "webpack");
        moduleCount = stats.compilation.modules.size;
      } finally {
        await tracing.close(compiler);
      }

      return { entryFile: join(outputDir, "main.js"), moduleCount };
    }
  },

  rspack: {
    name: "rspack",
    supportsWebpackLoaders: true,
    versionSource: () => `@rspack/core@${packageVersion("@rspack/core")}`,
    async watchBuild({ fixture, outputDir, mutateAfterInitialBuild }) {
      const rspackModule = await import("@rspack/core");
      const rspack = rspackModule.rspack ?? rspackModule.default;
      const tracing = createWebpackLikePhaseTracing({
        bundler: "rspack",
        spanPrefix: "Rspack",
        fixture,
        phase: "watch",
        persistentCache: false,
        cacheReadonly: false
      });
      const compiler = rspack({
        ...createRspackBenchmarkConfig({
          fixture,
          outputDir,
          cacheDir: join(outputDir, ".unused-cache"),
          persistentCache: false
        }),
        mode: "development",
        cache: true,
        plugins: [tracing.plugin]
      });
      const rebuildMs = await runCompilerWatchBuild({
        compiler,
        closeCompiler: () => tracing.close(compiler),
        mutateAfterInitialBuild,
        label: "Rspack",
        validate: ({ err, stats }) => {
          if (err) throw err;
          assertWebpackStats(stats, "Rspack");
        }
      });
      return { entryFile: join(outputDir, "main.js"), rebuildMs };
    },
    async build({
      fixture,
      outputDir,
      cacheDir,
      phase,
      persistentCache = true,
      cacheReadonly = false
    }) {
      const rspackModule = await import("@rspack/core");
      const rspack = rspackModule.rspack ?? rspackModule.default;
      const tracing = createWebpackLikePhaseTracing({
        bundler: "rspack",
        spanPrefix: "Rspack",
        fixture,
        phase,
        persistentCache,
        cacheReadonly
      });
      const compiler = rspack({
        ...createRspackBenchmarkConfig({
          fixture,
          outputDir,
          cacheDir,
          persistentCache,
          cacheReadonly
        }),
        plugins: [tracing.plugin]
      });

      let moduleCount;
      try {
        const stats = await runWebpackCompiler(compiler);
        assertWebpackStats(stats, "Rspack");
        moduleCount = stats.compilation.modules.size;
      } finally {
        await tracing.close(compiler);
      }

      return { entryFile: join(outputDir, "main.js"), moduleCount };
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

      let output;
      try {
        output = await bundle.write({
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

      const moduleCount = new Set(
        output.output.flatMap((item) =>
          item.type === "chunk" ? Object.keys(item.modules) : []
        )
      ).size;
      return { entryFile: join(outputDir, "main.js"), moduleCount };
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
    outputDir: ({ fixture, baseDir }) =>
      fixture.requiresWebpackLoaders
        ? join(baseDir, "turbopack-loader-fixture", "dist")
        : join(fixture.context, "dist"),
    versionSource: ({ options }) => {
      if (options.turbopackBinary) {
        return `hardfist/bundler-diff@${options.turbopackCommit ?? "turbopack-cli-main"}+release-turbopack-cli`;
      }
      return `vercel/next.js@${options.turbopackCommit ?? DEFAULT_TURBOPACK_COMMIT}+benchmark-cache-flush`;
    },
    async prepareBuild({ fixture, outputDir, phase }) {
      if (!fixture.requiresWebpackLoaders || phase === "warm") {
        return;
      }
      const context = dirname(outputDir);
      await rm(context, { recursive: true, force: true });
      await cp(fixture.context, context, { recursive: true });
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
    async build({ fixture, outputDir, cacheDir, phase, persistentCache = true, options }) {
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
      const context = fixture.requiresWebpackLoaders
        ? dirname(outputDir ?? join(fixture.context, "dist"))
        : fixture.context;
      await transformTurbopackLoaderFixture({ fixture, context, phase });
      const args = [
        "build",
        "--dir",
        context,
        "--root",
        context,
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
      const traceSourcePath = join(context, ".turbopack", "trace.log");
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

      return { entryFile: join(context, "dist/index.entry.js") };
    }
  }
};

export function createRspackBenchmarkConfig({
  fixture,
  outputDir,
  cacheDir,
  persistentCache = true,
  cacheReadonly = false
}) {
  const baseConfig = webpackLikeConfig({ fixture, outputDir });
  return {
    ...baseConfig,
    resolve: rspackResolveConfig(),
    externalsPresets: {
      node: false
    },
    output: {
      ...baseConfig.output,
      module: false,
      iife: false,
      chunkFormat: "commonjs",
      chunkLoading: "require",
      workerChunkLoading: false,
      wasmLoading: false,
      workerWasmLoading: false,
      asyncChunks: true,
      pathinfo: false,
      strictModuleErrorHandling: false,
      compareBeforeEmit: false
    },
    module: {
      ...(baseConfig.module ?? {}),
      parser: {
        javascript: {
          commonjs: false,
          commonjsMagicComments: false,
          createRequire: false,
          exportsPresence: false,
          importDynamic: true,
          importMeta: false,
          importMetaResolve: false,
          requireAlias: false,
          requireAsExpression: false,
          requireDynamic: false,
          requireResolve: false,
          url: false,
          worker: false
        }
      }
    },
    amd: false,
    node: false,
    performance: false,
    experiments: {
      asyncWebAssembly: false
    },
    optimization: {
      moduleIds: "named",
      chunkIds: "named",
      minimize: false,
      mergeDuplicateChunks: false,
      splitChunks: false,
      runtimeChunk: false,
      removeEmptyChunks: true,
      realContentHash: false,
      sideEffects: true,
      providedExports: true,
      concatenateModules: false,
      innerGraph: false,
      usedExports: false,
      mangleExports: false,
      inlineExports: false,
      nodeEnv: false,
      emitOnErrors: true,
      avoidEntryIife: false
    },
    cache: persistentCache
      ? {
          type: "persistent",
          storage: {
            type: "filesystem",
            directory: cacheDir
          },
          snapshot: {
            managedPaths: [RSPACK_PACKAGE_ROOT]
          },
          readonly: cacheReadonly
        }
      : false
  };
}

function rspackResolveConfig() {
  return {
    ...unpackResolvePolicy(),
    byDependency: {
      esm: unpackResolvePolicy()
    }
  };
}

function unpackResolvePolicy() {
  return {
    conditionNames: [],
    extensions: [".ts", ".tsx", ".js", ".jsx"],
    mainFields: ["main"]
  };
}

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

function webpackLikeConfig({ fixture, outputDir, mode = "none" }) {
  const config = {
    mode,
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
    swcLoaderRule(/\.js$/, "ecmascript"),
    swcLoaderRule(/\.ts$/, "typescript")
  ];
}

function swcLoaderRule(test, syntax) {
  return {
    test,
    loader: SWC_LOADER,
    options: swcLoaderOptions(syntax)
  };
}

function swcLoaderOptions(syntax) {
  return {
    jsc: {
      parser: { syntax },
      target: "es2022"
    },
    module: { type: "es6" },
    sourceMaps: false
  };
}

async function transformTurbopackLoaderFixture({ fixture, context, phase }) {
  if (!fixture.requiresWebpackLoaders) {
    return;
  }

  const manifestPath = join(context, ".swc-loader-manifest.json");
  const reset = phase !== "warm";
  const previous = reset ? {} : await readJsonIfPresent(manifestPath);
  const next = {};
  for (const resourcePath of await javascriptFiles(fixture.context)) {
    const fixturePath = relative(fixture.context, resourcePath);
    const syntax = resourcePath.endsWith(".ts") ? "typescript" : "ecmascript";
    const source = await readFile(resourcePath, "utf8");
    const sourceHash = createHash("sha256").update(source).digest("hex");
    next[fixturePath] = sourceHash;
    if (!reset && previous[fixturePath] === sourceHash) {
      continue;
    }
    const transformed = await runSwcLoader({
      source,
      resourcePath,
      rootContext: fixture.context,
      options: swcLoaderOptions(syntax)
    });
    const targetPath = join(context, fixturePath);
    await mkdir(dirname(targetPath), { recursive: true });
    await writeFile(targetPath, transformed, "utf8");
  }
  await writeFile(manifestPath, `${JSON.stringify(next, null, 2)}\n`, "utf8");
}

async function readJsonIfPresent(path) {
  try {
    return JSON.parse(await readFile(path, "utf8"));
  } catch (error) {
    if (error?.code === "ENOENT") {
      return {};
    }
    throw error;
  }
}

async function javascriptFiles(root) {
  const files = [];

  async function visit(directory) {
    const entries = await readdir(directory, { withFileTypes: true });
    for (const entry of entries) {
      if (entry.name === "dist" || entry.name === ".turbopack") {
        continue;
      }
      const path = join(directory, entry.name);
      if (entry.isDirectory()) {
        await visit(path);
      } else if (entry.isFile() && (entry.name.endsWith(".js") || entry.name.endsWith(".ts"))) {
        files.push(path);
      }
    }
  }

  await visit(root);
  files.sort();
  return files;
}

function runSwcLoader({ source, resourcePath, rootContext, options }) {
  const loader = require(SWC_LOADER);

  return new Promise((resolve, reject) => {
    let completed = false;
    const callback = (error, result) => {
      if (completed) {
        return;
      }
      completed = true;
      if (error) {
        reject(error);
      } else if (typeof result !== "string") {
        reject(new TypeError(`swc-loader returned ${typeof result} for ${resourcePath}`));
      } else {
        resolve(result);
      }
    };
    const context = {
      resourcePath,
      rootContext,
      sourceMap: false,
      getOptions: () => options,
      async: () => callback,
      callback,
      emitError: reject,
      emitWarning() {},
      getLogger: () => ({ debug() {}, info() {}, log() {}, warn() {}, error() {} })
    };

    try {
      const result = loader.call(context, source);
      if (typeof result === "string" && !completed) {
        completed = true;
        resolve(result);
      } else if (result?.then) {
        result.then((value) => callback(null, value), callback);
      }
    } catch (error) {
      callback(error);
    }
  });
}

function assertNoWebpackLoaderFixture(fixture, bundler) {
  if (!fixture.requiresWebpackLoaders) {
    return;
  }

  throw unsupported(`${bundler} does not support the loader benchmark fixture`);
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

function closeWatching(watching) {
  return new Promise((resolve, reject) => {
    watching.close((err) => (err ? reject(err) : resolve()));
  });
}

async function runCompilerWatchBuild({
  compiler,
  closeCompiler = () => closeWebpackCompiler(compiler),
  mutateAfterInitialBuild,
  label,
  validate
}) {
  const rebuild = createWatchRebuild({ mutateAfterInitialBuild, label, validate });
  const watching = compiler.watch({ aggregateTimeout: 0 }, rebuild.handler);
  try {
    return await rebuild.promise;
  } finally {
    await closeWatching(watching);
    await closeCompiler();
  }
}

function createWatchRebuild({ mutateAfterInitialBuild, label, validate }) {
  let handler;
  const promise = new Promise((resolve, reject) => {
    let state = "initial";
    let rebuildStarted;
    let initialHash;
    handler = async (err, stats) => {
      try {
        validate({ err, stats });
        if (state === "initial") {
          state = "mutating";
          initialHash = stats?.hash;
          // Let the compiler finish installing its watch subscriptions before mutation.
          await new Promise((resolve) => setTimeout(resolve, 25));
          rebuildStarted = performance.now();
          await mutateAfterInitialBuild();
          state = "waiting";
          return;
        }
        if (state === "waiting") {
          if (initialHash && stats?.hash === initialHash) {
            return;
          }
          state = "done";
          resolve(Number((performance.now() - rebuildStarted).toFixed(3)));
        }
      } catch (error) {
        reject(new Error(`${label} watch build failed: ${error.message}`, { cause: error }));
      }
    };
  });
  return { handler, promise };
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

function createWebpackLikePhaseTracing({
  bundler,
  spanPrefix,
  fixture,
  phase,
  persistentCache,
  cacheReadonly
}) {
  const pluginName = "UnpackBenchmarkWebpackLikePhaseTracingPlugin";
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
        `[${bundler} tracing]`,
        `fixture=${fixture.name}`,
        `phase=${phase}`,
        `persistent_cache=${persistentCache ? "on" : "off"}`,
        `cache_readonly=${cacheReadonly ? "on" : "off"}`
      ].join(" ") + "\n"
    );

    for (const [span, duration] of [
      [`${spanPrefix}::run`, durations.get("compilerRun")],
      [`${spanPrefix}::make`, durations.get("make")],
      [`${spanPrefix}::build_chunk_graph`, durations.get("chunkGraph")],
      [`${spanPrefix}::create_assets`, durations.get("createAssets")],
      [`${spanPrefix}::emit_assets`, durations.get("emitAssets")],
      [`${spanPrefix}::flush_cache`, durations.get("flushCache")]
    ]) {
      if (duration === undefined) {
        continue;
      }
      process.stderr.write(
        `TRACE ${span}: ${bundler}: close time.busy=${formatDurationMs(duration)} time.idle=0ms\n`
      );
    }
  }

  function hasHook(hooks, name) {
    return Boolean(hooks && Object.prototype.hasOwnProperty.call(hooks, name));
  }

  function tap(hooks, name, callback) {
    if (!hasHook(hooks, name)) {
      return false;
    }
    hooks[name].tap(pluginName, callback);
    return true;
  }

  return {
    plugin: {
      apply(compiler) {
        tap(compiler.hooks, "beforeRun", () => start("compilerRun"));
        tap(compiler.hooks, "run", () => start("compilerRun"));
        tap(compiler.hooks, "watchRun", () => start("compilerRun"));
        tap(compiler.hooks, "done", () => end("compilerRun"));
        tap(compiler.hooks, "make", () => start("make"));
        tap(compiler.hooks, "finishMake", () => end("make"));
        tap(compiler.hooks, "emit", () => start("emitAssets"));
        tap(compiler.hooks, "afterEmit", () => end("emitAssets"));
        tap(compiler.hooks, "compilation", (compilation) => {
          tap(compilation.hooks, "beforeChunks", () => start("chunkGraph"));
          tap(compilation.hooks, "afterChunks", () => end("chunkGraph"));
          if (hasHook(compilation.hooks, "beforeCodeGeneration")) {
            tap(compilation.hooks, "beforeCodeGeneration", () => start("createAssets"));
          } else {
            tap(compilation.hooks, "processAssets", () => start("createAssets"));
          }
          tap(compilation.hooks, "afterProcessAssets", () => end("createAssets"));
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
