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
  sourcemap?: boolean;
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
  assetRender: CacheItemWorkObservation;
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
  options: { cwd?: string } = {}
): Promise<CacheProcessObservation> {
  const driver = fileURLToPath(new URL("./cache-process-driver.js", import.meta.url));
  const { stdout, stderr } = await execFile(
    process.execPath,
    [driver, JSON.stringify(request)],
    {
      cwd: options.cwd,
      maxBuffer: 1024 * 1024,
      env: {
        ...process.env,
        UNPACK_INTERNAL_TRACING: "unpack_core::cache_work=info"
      }
    }
  );
  const output = stdout.trim().split("\n").at(-1);
  if (!output) {
    throw new Error(`cache process produced no observation${stderr ? `: ${stderr}` : ""}`);
  }
  return {
    ...(JSON.parse(output) as CacheProcessObservation),
    cacheWork: parseCacheWork(stderr)
  };
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
  const item = (prefix: "resolve" | "module" | "asset_render") => ({
    hits: field(`${prefix}_hits`),
    misses: field(`${prefix}_misses`),
    stores: field(`${prefix}_stores`),
    restores: field(`${prefix}_restores`),
    evictions: field(`${prefix}_evictions`)
  });
  return {
    resolve: item("resolve"),
    moduleBuild: item("module"),
    assetRender: item("asset_render")
  };
}
