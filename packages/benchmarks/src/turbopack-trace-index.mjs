#!/usr/bin/env node

import { mkdir, readdir, stat, writeFile } from "node:fs/promises";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const PHASE_ORDER = new Map([
  ["cold", 0],
  ["warm", 1],
  ["no-cache", 2]
]);

export async function collectTurbopackTraces(rootDir) {
  const files = await walk(rootDir);
  const rows = [];

  for (const path of files) {
    if (
      !path.endsWith(`${sep}trace.log`) &&
      path !== join(rootDir, "trace.log")
    ) {
      continue;
    }

    const traceStat = await stat(path);
    if (!traceStat.isFile() || traceStat.size === 0) {
      continue;
    }

    const relativePath = relative(rootDir, path);
    const parts = relativePath.split(sep);
    const fixture = parts.length >= 3 ? parts.at(-3) : "";
    const phase = parts.length >= 2 ? parts.at(-2) : "";

    rows.push({
      fixture,
      phase,
      path,
      relativePath,
      bytes: traceStat.size
    });
  }

  rows.sort((left, right) => {
    const fixture = left.fixture.localeCompare(right.fixture);
    if (fixture !== 0) {
      return fixture;
    }
    const leftPhase = PHASE_ORDER.get(left.phase) ?? Number.MAX_SAFE_INTEGER;
    const rightPhase = PHASE_ORDER.get(right.phase) ?? Number.MAX_SAFE_INTEGER;
    if (leftPhase !== rightPhase) {
      return leftPhase - rightPhase;
    }
    return left.phase.localeCompare(right.phase);
  });

  return rows;
}

export function toTurbopackTraceMarkdown(rows, options = {}) {
  if (rows.length === 0) {
    return "No Turbopack trace files were captured.\n";
  }

  const linkBaseDir = options.linkBaseDir ?? options.rootDir;
  const artifactSentence = options.artifactUrl
    ? `Download [cross-bundler-benchmark-results](${escapeMarkdownUrl(options.artifactUrl)}) from the workflow run, then open \`turbopack-traces/index.html\` or use one of the artifact-local trace paths below.`
    : "Raw Turbopack trace files are uploaded in the workflow artifact. After downloading the artifact, open `turbopack-traces/index.html` or use one of the artifact-local trace paths below.";
  const lines = [
    "## Turbopack Trace Files",
    "",
    artifactSentence,
    "",
    "Start a trace server with `pnpm next internal trace <trace.log>` or `cargo run --bin turbo-trace-server --release -- <trace.log>`, then open https://trace.nextjs.org/.",
    "",
    "| fixture | build | trace file | size |",
    "| --- | --- | --- | ---: |"
  ];

  for (const row of rows) {
    const tracePath = linkBaseDir ? relative(linkBaseDir, row.path) : row.relativePath;
    lines.push(
      [
        row.fixture,
        row.phase,
        formatMarkdownCode(toHrefPath(tracePath)),
        formatBytes(row.bytes)
      ].join(" | ").replace(/^/, "| ").replace(/$/, " |")
    );
  }

  return `${lines.join("\n")}\n`;
}

export function toTurbopackTraceHtml(rows, options = {}) {
  const title = "Turbopack Trace Files";
  const linkBaseDir = options.linkBaseDir ?? options.rootDir;
  const largest = Math.max(...rows.map((row) => row.bytes), 1);
  const generatedAt = new Date().toISOString();
  const rowsHtml =
    rows.length === 0
      ? `<p class="empty">No Turbopack trace files were captured.</p>`
      : rows
          .map((row) => {
            const linkPath = linkBaseDir
              ? relative(linkBaseDir, row.path)
              : row.relativePath;
            const width = Math.max(2, (row.bytes / largest) * 100);
            return `<section class="trace">
  <div class="trace-main">
    <div>
      <p class="label">${escapeHtml(row.fixture)} / ${escapeHtml(row.phase)}</p>
      <a href="${escapeAttribute(toHrefPath(linkPath))}">${escapeHtml(row.relativePath)}</a>
    </div>
    <strong>${escapeHtml(formatBytes(row.bytes))}</strong>
  </div>
  <div class="bar" aria-label="${escapeAttribute(formatBytes(row.bytes))}">
    <span style="width: ${width.toFixed(2)}%"></span>
  </div>
</section>`;
          })
          .join("\n");

  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${title}</title>
  <style>
    :root {
      color-scheme: light;
      --bg: #f7f8fa;
      --panel: #ffffff;
      --ink: #172026;
      --muted: #61707d;
      --line: #d8dee5;
      --accent: #11745f;
      --accent-soft: #d9f0e9;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      background: var(--bg);
      color: var(--ink);
      font: 14px/1.5 system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    main {
      width: min(100% - 32px, 1040px);
      margin: 32px auto;
    }
    header {
      margin-bottom: 24px;
    }
    h1 {
      margin: 0 0 8px;
      font-size: 28px;
      line-height: 1.15;
      letter-spacing: 0;
    }
    p {
      margin: 0;
      color: var(--muted);
    }
    code {
      padding: 2px 5px;
      border: 1px solid var(--line);
      border-radius: 4px;
      background: #fff;
      color: var(--ink);
    }
    .viewer {
      margin: 18px 0 24px;
      padding: 16px;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: var(--panel);
    }
    .viewer p + p {
      margin-top: 8px;
    }
    .trace {
      margin: 10px 0;
      padding: 14px;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: var(--panel);
    }
    .trace-main {
      display: flex;
      gap: 16px;
      align-items: flex-start;
      justify-content: space-between;
    }
    .label {
      margin-bottom: 4px;
      color: var(--muted);
      font-size: 12px;
      text-transform: uppercase;
    }
    a {
      color: var(--accent);
      overflow-wrap: anywhere;
      text-decoration-thickness: 1px;
      text-underline-offset: 3px;
    }
    strong {
      white-space: nowrap;
    }
    .bar {
      height: 8px;
      margin-top: 12px;
      overflow: hidden;
      border-radius: 999px;
      background: var(--accent-soft);
    }
    .bar span {
      display: block;
      height: 100%;
      border-radius: inherit;
      background: var(--accent);
    }
    .empty {
      padding: 16px;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: var(--panel);
    }
  </style>
</head>
<body>
  <main>
    <header>
      <h1>${title}</h1>
      <p>Generated at ${escapeHtml(generatedAt)}.</p>
    </header>
    <section class="viewer">
      <p>Start a trace server with <code>pnpm next internal trace &lt;trace.log&gt;</code> or <code>cargo run --bin turbo-trace-server --release -- &lt;trace.log&gt;</code>.</p>
      <p>Open <a href="https://trace.nextjs.org/">trace.nextjs.org</a> while the trace server is running.</p>
    </section>
    ${rowsHtml}
  </main>
</body>
</html>
`;
}

function formatBytes(bytes) {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KiB", "MiB", "GiB"];
  let value = bytes / 1024;
  for (const unit of units) {
    if (value < 1024 || unit === units.at(-1)) {
      return `${value.toFixed(value < 10 ? 2 : 1)} ${unit}`;
    }
    value /= 1024;
  }
  return `${bytes} B`;
}

async function walk(rootDir) {
  const entries = await readdir(rootDir, { withFileTypes: true }).catch((error) => {
    if (error?.code === "ENOENT") {
      return [];
    }
    throw error;
  });
  const paths = [];

  for (const entry of entries) {
    const path = join(rootDir, entry.name);
    if (entry.isDirectory()) {
      paths.push(...await walk(path));
    } else if (entry.isFile()) {
      paths.push(path);
    }
  }

  return paths;
}

function formatMarkdownCode(value) {
  return `\`${String(value).replaceAll("`", "\\`").replaceAll("|", "\\|")}\``;
}

function escapeMarkdownUrl(value) {
  return String(value).replaceAll(")", "%29");
}

function toHrefPath(value) {
  return String(value).split(sep).join("/");
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function escapeAttribute(value) {
  return escapeHtml(value).replaceAll("\"", "&quot;");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { traceDir, markdownPath, htmlPath, artifactUrl } = parseCliArgs(
    process.argv.slice(2)
  );
  if (!traceDir || !markdownPath) {
    process.stderr.write(
      "Usage: node src/turbopack-trace-index.mjs <trace-dir> <output-md> [--html <output-html>] [--artifact-url <url>]\n"
    );
    process.exit(1);
  }

  const rows = await collectTurbopackTraces(traceDir);
  await mkdir(dirname(markdownPath), { recursive: true });
  await writeFile(
    markdownPath,
    toTurbopackTraceMarkdown(rows, {
      rootDir: traceDir,
      linkBaseDir: dirname(markdownPath),
      artifactUrl
    }),
    "utf8"
  );

  if (htmlPath) {
    await mkdir(dirname(htmlPath), { recursive: true });
    await writeFile(
      htmlPath,
      toTurbopackTraceHtml(rows, {
        rootDir: traceDir,
        linkBaseDir: dirname(htmlPath)
      }),
      "utf8"
    );
  }
}

function parseCliArgs(args) {
  const parsed = {
    traceDir: args[0],
    markdownPath: args[1],
    htmlPath: undefined,
    artifactUrl: undefined
  };

  for (let index = 2; index < args.length; index += 1) {
    const arg = args[index];
    switch (arg) {
      case "--html":
        index += 1;
        if (index >= args.length) {
          throw new Error("--html requires a value");
        }
        parsed.htmlPath = args[index];
        break;
      case "--artifact-url":
        index += 1;
        if (index >= args.length) {
          throw new Error("--artifact-url requires a value");
        }
        parsed.artifactUrl = args[index];
        break;
      default:
        throw new Error(`unknown argument '${arg}'`);
    }
  }

  return parsed;
}
