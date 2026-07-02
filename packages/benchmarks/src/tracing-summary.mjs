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

export function toUnpackTracingSummaryMarkdown(rows) {
  if (rows.length === 0) {
    return "No Unpack tracing phase timings were captured.\n";
  }

  const lines = [
    "Durations are Rust tracing span close elapsed times.",
    "",
    [
      "fixture",
      "build",
      ...PHASE_COLUMNS.map(([, label]) => `${label} ms`)
    ].join(" | ").replace(/^/, "| ").replace(/$/, " |"),
    [
      "---",
      "---",
      ...PHASE_COLUMNS.map(() => "---:")
    ].join(" | ").replace(/^/, "| ").replace(/$/, " |")
  ];

  for (const row of rows) {
    lines.push(
      [
        row.fixture,
        row.build,
        ...PHASE_COLUMNS.map(([key]) => formatMs(row[key]))
      ].join(" | ").replace(/^/, "| ").replace(/$/, " |")
    );
  }

  return `${lines.join("\n")}\n`;
}

function parseTracingHeader(line) {
  if (!line.startsWith("[unpack tracing]")) {
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
  return { fixture, phase };
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
  if (name === "Compiler::run") {
    return "compilerRun";
  }
  if (name.includes("Compilation::make")) {
    return "make";
  }
  if (name.includes("Compilation::build_chunk_graph")) {
    return "chunkGraph";
  }
  if (name.includes("Compilation::create_assets")) {
    return "createAssets";
  }
  if (name === "unpack_node::emit_assets") {
    return "emitAssets";
  }
  if (name === "Compiler::flush_cache") {
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
  const [, , inputPath, outputPath] = process.argv;
  if (!inputPath || !outputPath) {
    process.stderr.write("Usage: node src/tracing-summary.mjs <input-log> <output-md>\n");
    process.exit(1);
  }

  const log = await readFile(inputPath, "utf8").catch(() => "");
  const markdown = toUnpackTracingSummaryMarkdown(parseUnpackTracingSummary(log));
  await writeFile(outputPath, markdown, "utf8");
}
