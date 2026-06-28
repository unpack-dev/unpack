import { copyFileSync, mkdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageDir = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = resolve(packageDir, "../..");
const profile = process.env.UNPACK_NATIVE_PROFILE === "release" ? "release" : "debug";
const cargoArgs = ["build", "-p", "unpack_node"];

if (profile === "release") {
  cargoArgs.push("--release");
}

const result = spawnSync("cargo", cargoArgs, {
  cwd: repoRoot,
  stdio: "inherit"
});

if (result.status !== 0) {
  process.exit(result.status ?? 1);
}

const libraryName =
  process.platform === "darwin"
    ? "libunpack_node.dylib"
    : process.platform === "linux"
      ? "libunpack_node.so"
      : process.platform === "win32"
        ? "unpack_node.dll"
        : undefined;

if (!libraryName) {
  throw new Error(`unsupported platform for native build: ${process.platform}`);
}

const distDir = resolve(packageDir, "dist");
mkdirSync(distDir, { recursive: true });
copyFileSync(
  resolve(repoRoot, "target", profile, libraryName),
  resolve(distDir, "unpack_node.node")
);
