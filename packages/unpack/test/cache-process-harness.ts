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
  outputPath: string | null;
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
      maxBuffer: 1024 * 1024
    }
  );
  const output = stdout.trim().split("\n").at(-1);
  if (!output) {
    throw new Error(`cache process produced no observation${stderr ? `: ${stderr}` : ""}`);
  }
  return JSON.parse(output) as CacheProcessObservation;
}
