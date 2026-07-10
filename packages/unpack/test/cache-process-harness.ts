import { execFile as execFileCallback } from "node:child_process";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFile = promisify(execFileCallback);

export type CacheProcessBundler = "unpack" | "webpack";

export interface CacheProcessOptions {
  context: string;
  entry?: string;
  mode?: "development" | "production" | "none";
  name?: string;
  outputPath: string;
<<<<<<< HEAD
  sourcemap?: boolean;
=======
>>>>>>> origin/main
  cache?: boolean | Record<string, unknown>;
  snapshot?: Record<string, unknown>;
}

export interface CacheProcessRequest {
  bundler: CacheProcessBundler;
  options: CacheProcessOptions;
}

export interface CacheProcessObservation {
  pid: number;
  instanceId: string;
  synchronousError: boolean;
  error: { name: string; message: string } | null;
  hasStats: boolean;
  hasErrors: boolean | null;
  assets: string[];
<<<<<<< HEAD
  assetDetails: { name: string; size: number }[];
  outputPath: string | null;
  cacheWork: CacheWorkObservation | null;
}

export interface CacheItemWorkObservation {
  hits: number;
  misses: number;
  stores: number;
  restores: number;
  evictions: number;
}

export interface CacheWorkObservation {
  resolve: CacheItemWorkObservation;
  moduleBuild: CacheItemWorkObservation;
  codeGeneration: CacheItemWorkObservation;
  assetRender: CacheItemWorkObservation;
}

export interface CacheProcessTermination {
  code: number | null;
  signal: string | null;
=======
  outputPath: string | null;
>>>>>>> origin/main
}

export async function runColdWarmBuilds(
  request: CacheProcessRequest,
  options: { cwd?: string } = {}
) {
  const cold = await runCacheProcess(request, options);
  const warm = await runCacheProcess(request, options);
  return { cold, warm };
}

export async function runCacheProcess(
  request: CacheProcessRequest,
<<<<<<< HEAD
  options: { cwd?: string; env?: NodeJS.ProcessEnv } = {}
=======
  options: { cwd?: string } = {}
>>>>>>> origin/main
): Promise<CacheProcessObservation> {
  const driver = fileURLToPath(new URL("./cache-process-driver.js", import.meta.url));
  const { stdout, stderr } = await execFile(
    process.execPath,
    [driver, JSON.stringify(request)],
    {
      cwd: options.cwd,
<<<<<<< HEAD
      maxBuffer: 1024 * 1024,
      env: {
        ...process.env,
        ...options.env,
        UNPACK_INTERNAL_TRACING: "unpack_core::cache_work=info"
      }
=======
      maxBuffer: 1024 * 1024
>>>>>>> origin/main
    }
  );
  const output = stdout.trim().split("\n").at(-1);
  if (!output) {
    throw new Error(`cache process produced no observation${stderr ? `: ${stderr}` : ""}`);
  }
<<<<<<< HEAD
  return {
    ...(JSON.parse(output) as CacheProcessObservation),
    cacheWork: parseCacheWork(stderr)
  };
}

/**
 * Runs a cache process that is expected to be force-stopped by a native
 * publication fault hook. Keeping this separate from the normal observation
 * path ensures acceptance tests cannot accidentally accept a graceful error.
 */
export async function runCacheProcessExpectTermination(
  request: CacheProcessRequest,
  options: { cwd?: string; env?: NodeJS.ProcessEnv } = {}
): Promise<CacheProcessTermination> {
  try {
    await runCacheProcess(request, options);
  } catch (error) {
    const termination = error as { code?: unknown; signal?: unknown };
    const code = typeof termination.code === "number" ? termination.code : null;
    const signal =
      typeof termination.signal === "string" ? termination.signal : null;
    if (code !== null || signal !== null) {
      return { code, signal };
    }
    throw error;
  }
  throw new Error("cache process completed instead of being force-stopped");
}

function parseCacheWork(stderr: string): CacheWorkObservation | null {
  const line = stderr
    .trim()
    .split("\n")
    .findLast((candidate) => candidate.includes("cache_work"));
  if (line === undefined) {
    return null;
  }
  const field = (name: string) => {
    const value = new RegExp(`(?:^|\\s)${name}=(\\d+)`).exec(line)?.[1];
    if (value === undefined) {
      throw new Error(`cache work trace is missing ${name}: ${line}`);
    }
    return Number(value);
  };
  const item = (
    prefix: "resolve" | "module" | "code_generation" | "asset_render"
  ) => ({
    hits: field(`${prefix}_hits`),
    misses: field(`${prefix}_misses`),
    stores: field(`${prefix}_stores`),
    restores: field(`${prefix}_restores`),
    evictions: field(`${prefix}_evictions`)
  });
  return {
    resolve: item("resolve"),
    moduleBuild: item("module"),
    codeGeneration: item("code_generation"),
    assetRender: item("asset_render")
  };
=======
  return JSON.parse(output) as CacheProcessObservation;
>>>>>>> origin/main
}
