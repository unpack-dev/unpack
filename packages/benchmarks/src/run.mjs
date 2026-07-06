#!/usr/bin/env node

import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

import {
  DEFAULT_BUNDLERS,
  DEFAULT_FIXTURES,
  DEFAULT_TURBOPACK_COMMIT,
  runBenchmark,
  toSummaryMarkdown
} from "./runner.mjs";

const invocationCwd = process.env.INIT_CWD ?? process.cwd();
const options = parseArgs(process.argv.slice(2));
const report = await runBenchmark(options);
const summary = toSummaryMarkdown(report);

if (options.outputJson) {
  await writeOutput(options.outputJson, `${JSON.stringify(report, null, 2)}\n`);
}

if (options.summaryMd) {
  await writeOutput(options.summaryMd, summary);
}

process.stdout.write(summary);

function parseArgs(args) {
  const parsed = {
    workspaceDir: resolve(invocationCwd, ".benchmark-work"),
    fixtures: DEFAULT_FIXTURES,
    bundlers: DEFAULT_BUNDLERS,
    turbopackCommit: DEFAULT_TURBOPACK_COMMIT,
    turbopackProfile: "release"
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    const value = () => {
      index += 1;
      if (index >= args.length) {
        throw new Error(`${arg} requires a value`);
      }
      return args[index];
    };

    switch (arg) {
      case "--":
        break;
      case "--workspace":
        parsed.workspaceDir = resolve(invocationCwd, value());
        break;
      case "--fixtures":
        parsed.fixtures = splitList(value());
        break;
      case "--bundlers":
        parsed.bundlers = splitList(value());
        break;
      case "--output-json":
        parsed.outputJson = resolve(invocationCwd, value());
        break;
      case "--summary-md":
        parsed.summaryMd = resolve(invocationCwd, value());
        break;
      case "--unpack-tracing":
        parsed.unpackTracing = value();
        break;
      case "--no-unpack-tracing":
        parsed.unpackTracing = false;
        break;
      case "--turbopack-repo":
        parsed.turbopackRepo = resolve(invocationCwd, value());
        break;
      case "--turbopack-commit":
        parsed.turbopackCommit = value();
        break;
      case "--turbopack-profile":
        parsed.turbopackProfile = value();
        break;
      case "--help":
        printHelp();
        process.exit(0);
        break;
      default:
        throw new Error(`unknown argument '${arg}'`);
    }
  }

  return parsed;
}

function splitList(value) {
  return value.split(",").map((item) => item.trim()).filter(Boolean);
}

async function writeOutput(path, contents) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, contents, "utf8");
}

function printHelp() {
  process.stdout.write(`Usage: pnpm --filter @unpack-js/benchmarks bench -- [options]

Options:
  --workspace <path>            Benchmark-owned workspace directory
  --fixtures <list>             Comma-separated fixture list (default: large)
  --bundlers <list>             Comma-separated bundler list
  --output-json <path>          Write raw JSON report
  --summary-md <path>           Write Markdown summary table
  --unpack-tracing <filter>     Set the Unpack tracing filter (default: unpack_core=trace,unpack_node=trace)
  --no-unpack-tracing           Do not print Unpack tracing details
  --turbopack-repo <path>       Fixed Next.js checkout containing turbopack/
  --turbopack-commit <sha>      Commit shown for Turbopack results
  --turbopack-profile <name>    Cargo profile for turbopack-cli (default: release)
`);
}
