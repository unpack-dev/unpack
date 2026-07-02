import { execFile as execFileCallback } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { promisify } from "node:util";

import { DEFAULT_TURBOPACK_COMMIT } from "./runner.mjs";

const execFile = promisify(execFileCallback);
const require = createRequire(import.meta.url);
const preparedTurbopackBuilds = new Set();

export const adapters = {
  unpack: {
    name: "unpack",
    versionSource: () => `@unpack-js/core@${packageVersion("@unpack-js/core")}`,
    async build({ fixture, outputDir, cacheDir, persistentCache = true, cacheReadonly = false }) {
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
    versionSource: () => `webpack@${packageVersion("webpack")}`,
    async build({ fixture, outputDir, cacheDir, persistentCache = true, cacheReadonly = false }) {
      const webpackModule = await import("webpack");
      const webpack = webpackModule.default ?? webpackModule;
      const compiler = webpack({
        ...webpackLikeConfig({ fixture, outputDir }),
        cache: persistentCache
          ? {
              type: "filesystem",
              cacheDirectory: cacheDir,
              readonly: cacheReadonly
            }
          : false
      });

      try {
        const stats = await runWebpackCompiler(compiler);
        assertWebpackStats(stats, "webpack");
      } finally {
        await closeWebpackCompiler(compiler);
      }

      return { entryFile: join(outputDir, "main.js") };
    }
  },

  rspack: {
    name: "rspack",
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
      const { rolldown } = await import("rolldown");
      const bundle = await rolldown({
        input: resolve(fixture.context, fixture.entry),
        treeshake: true
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

  turbopack: {
    name: "turbopack",
    outputDir: ({ fixture }) => join(fixture.context, "dist"),
    versionSource: ({ options }) =>
      `vercel/next.js@${options.turbopackCommit ?? DEFAULT_TURBOPACK_COMMIT}+benchmark-cache-flush`,
    async prepare({ options }) {
      const repo = options.turbopackRepo;
      if (!repo) {
        throw unsupported("Turbopack requires --turbopack-repo pointing at a fixed Next.js checkout");
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
    async build({ fixture, cacheDir, persistentCache = true, options }) {
      const repo = options.turbopackRepo;
      if (!repo) {
        throw unsupported("Turbopack requires --turbopack-repo pointing at a fixed Next.js checkout");
      }

      const profile = options.turbopackProfile ?? "release";
      const binary = join(
        repo,
        "target",
        profile === "dev" ? "debug" : profile,
        "turbopack-cli"
      );
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

      await execFile(
        binary,
        args,
        {
          cwd: repo,
          env: { ...process.env, CI: process.env.CI ?? "1" },
          timeout: 10 * 60 * 1000,
          maxBuffer: 1024 * 1024 * 20
        }
      );

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
  return {
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

function unsupported(message) {
  const error = new Error(message);
  error.code = "UNSUPPORTED_BUNDLER";
  return error;
}
