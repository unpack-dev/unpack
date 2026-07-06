import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  collectTurbopackTraces,
  toTurbopackTraceHtml,
  toTurbopackTraceMarkdown
} from "../src/turbopack-trace-index.mjs";

test("turbopack trace index lists traces by fixture and build phase", async () => {
  const workspace = await mkdtemp(join(tmpdir(), "unpack-benchmarks-"));

  try {
    const traceDir = join(workspace, "turbopack-traces");
    await mkdir(join(traceDir, "large", "warm"), { recursive: true });
    await mkdir(join(traceDir, "large", "cold"), { recursive: true });
    await mkdir(join(traceDir, "small", "cold"), { recursive: true });
    await writeFile(join(traceDir, "large", "warm", "trace.log"), "warm", "utf8");
    await writeFile(join(traceDir, "large", "cold", "trace.log"), "cold", "utf8");
    await writeFile(join(traceDir, "small", "cold", "trace.log"), "small", "utf8");

    const rows = await collectTurbopackTraces(traceDir);
    assert.deepEqual(
      rows.map((row) => `${row.fixture}:${row.phase}:${row.bytes}`),
      ["large:cold:4", "large:warm:4", "small:cold:5"]
    );

    const markdown = toTurbopackTraceMarkdown(rows, {
      rootDir: traceDir,
      linkBaseDir: workspace
    });
    assert.match(markdown, /## Turbopack Trace Files/);
    assert.match(
      markdown,
      /\| large \| cold \| `turbopack-traces\/large\/cold\/trace\.log` \| 4 B \|/
    );
    assert.doesNotMatch(markdown, /\]\(turbopack-traces\/large\/cold\/trace\.log\)/);

    const html = toTurbopackTraceHtml(rows, {
      rootDir: traceDir,
      linkBaseDir: traceDir
    });
    assert.match(html, /Turbopack Trace Files/);
    assert.match(html, /trace\.nextjs\.org/);
    assert.match(html, /large \/ cold/);
  } finally {
    await rm(workspace, { recursive: true, force: true });
  }
});

test("turbopack trace index reports when no traces were captured", () => {
  assert.equal(
    toTurbopackTraceMarkdown([]),
    "No Turbopack trace files were captured.\n"
  );
  assert.match(toTurbopackTraceHtml([]), /No Turbopack trace files were captured/);
});
