#!/usr/bin/env node

import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const PHASE_COLUMNS = [
  ["compilerRun", "compiler run"],
  ["make", "make"],
  ["chunkGraph", "chunk graph"],
  ["createAssets", "asset creation"],
  ["emitAssets", "emit assets"],
  ["flushCache", "flush cache"]
];

export function parseUnpackTracingSummary(log) {
  const rows = [];
  let current;

  for (const line of log.split(/\r?\n/)) {
    const header = parseTracingHeader(line);
    if (header) {
      current = {
        bundler: header.bundler,
        fixture: header.fixture,
        build: header.phase
      };
      rows.push(current);
      continue;
    }

    if (!current) {
      continue;
    }

    const span = parseSpanClose(line);
    if (!span) {
      continue;
    }

    const key = phaseKey(span.name);
    if (!key) {
      continue;
    }

    current[key] = (current[key] ?? 0) + span.durationMs;
  }

  return rows;
}

export function filterUnpackTracingSummaryRows(rows, options = {}) {
  if (!options.fixture) {
    return rows;
  }
  return rows.filter((row) => row.fixture === options.fixture);
}

export function toUnpackTracingSummaryMarkdown(rows, options = {}) {
  if (rows.length === 0) {
    const fixture = options.fixture ? ` for fixture \`${options.fixture}\`` : "";
    return `No bundler phase timings were captured${fixture}.\n`;
  }

  const lines = [
    "Durations are phase elapsed times captured by benchmark tracing.",
    "",
    [
      "fixture",
      "bundler",
      "build",
      ...PHASE_COLUMNS.map(([, label]) => `${label} ms`)
    ].join(" | ").replace(/^/, "| ").replace(/$/, " |"),
    [
      "---",
      "---",
      "---",
      ...PHASE_COLUMNS.map(() => "---:")
    ].join(" | ").replace(/^/, "| ").replace(/$/, " |")
  ];

  for (const row of rows) {
    lines.push(
      [
        row.fixture,
        row.bundler,
        row.build,
        ...PHASE_COLUMNS.map(([key]) => formatMs(row[key]))
      ].join(" | ").replace(/^/, "| ").replace(/$/, " |")
    );
  }

  return `${lines.join("\n")}\n`;
}

function parseTracingHeader(line) {
  const match = line.match(/^\[(unpack|webpack|rspack) tracing\]/);
  if (!match) {
    return null;
  }

  const fields = new Map();
  for (const match of line.matchAll(/\b([a-z_]+)=([^\s]+)/g)) {
    fields.set(match[1], match[2]);
  }

  const fixture = fields.get("fixture");
  const phase = fields.get("phase");
  if (!fixture || !phase) {
    return null;
  }
  return { bundler: match[1], fixture, phase };
}

function parseSpanClose(line) {
  const match = line.match(
    /\bTRACE\s+(.+):\s+\S+:\s+close\s+time\.busy=(\S+)\s+time\.idle=(\S+)/
  );
  if (!match) {
    return null;
  }

  const busyMs = parseDurationMs(match[2]);
  const idleMs = parseDurationMs(match[3]);
  if (busyMs === null || idleMs === null) {
    return null;
  }

  return {
    name: match[1],
    durationMs: busyMs + idleMs
  };
}

function phaseKey(name) {
  if (name === "Compiler::run" || name === "Webpack::run") {
    return "compilerRun";
  }
  if (name === "Rspack::run") {
    return "compilerRun";
  }
  if (
    name.includes("Compilation::make") ||
    name === "Webpack::make" ||
    name === "Rspack::make"
  ) {
    return "make";
  }
  if (
    name.includes("Compilation::build_chunk_graph") ||
    name === "Webpack::build_chunk_graph" ||
    name === "Rspack::build_chunk_graph"
  ) {
    return "chunkGraph";
  }
  if (
    name.includes("Compilation::create_assets") ||
    name === "Webpack::create_assets" ||
    name === "Rspack::create_assets"
  ) {
    return "createAssets";
  }
  if (
    name === "unpack_node::emit_assets" ||
    name === "Webpack::emit_assets" ||
    name === "Rspack::emit_assets"
  ) {
    return "emitAssets";
  }
  if (
    name === "Compiler::flush_cache" ||
    name === "Webpack::flush_cache" ||
    name === "Rspack::flush_cache"
  ) {
    return "flushCache";
  }
  return null;
}

function parseDurationMs(value) {
  const match = value.match(/^([0-9]+(?:\.[0-9]+)?)(ns|µs|us|ms|s)$/);
  if (!match) {
    return null;
  }

  const amount = Number(match[1]);
  switch (match[2]) {
    case "ns":
      return amount / 1_000_000;
    case "µs":
    case "us":
      return amount / 1_000;
    case "ms":
      return amount;
    case "s":
      return amount * 1_000;
    default:
      return null;
  }
}

function formatMs(value) {
  if (value === undefined) {
    return "";
  }
  return value.toFixed(3);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { inputPath, outputPath, fixture } = parseCliArgs(process.argv.slice(2));
  if (!inputPath || !outputPath) {
    process.stderr.write(
      "Usage: node src/tracing-summary.mjs <input-log> <output-md> [--fixture <name>]\n"
    );
    process.exit(1);
  }

  const log = await readFile(inputPath, "utf8").catch(() => "");
  const rows = filterUnpackTracingSummaryRows(parseUnpackTracingSummary(log), { fixture });
  const markdown = toUnpackTracingSummaryMarkdown(rows, { fixture });
  await writeFile(outputPath, markdown, "utf8");
}

function parseCliArgs(args) {
  const parsed = {
    inputPath: args[0],
    outputPath: args[1],
    fixture: undefined
  };

  for (let index = 2; index < args.length; index += 1) {
    const arg = args[index];
    switch (arg) {
      case "--fixture":
        index += 1;
        if (index >= args.length) {
          throw new Error("--fixture requires a value");
        }
        parsed.fixture = args[index];
        break;
      default:
        throw new Error(`unknown argument '${arg}'`);
    }
  }

  return parsed;
}
