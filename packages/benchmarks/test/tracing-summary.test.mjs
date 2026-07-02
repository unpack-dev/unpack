import assert from "node:assert/strict";
import test from "node:test";

import {
  filterUnpackTracingSummaryRows,
  parseUnpackTracingSummary,
  toUnpackTracingSummaryMarkdown
} from "../src/tracing-summary.mjs";

test("tracing summary groups major phase timings by fixture and build phase", () => {
  const rows = parseUnpackTracingSummary(`[unpack tracing] fixture=small phase=cold persistent_cache=on cache_readonly=off filter=unpack_core=trace,unpack_node=trace
2026-07-02T10:52:55.739797Z TRACE Compiler::run:Compilation::make: unpack_core::compilation: close time.busy=5.87ms time.idle=9.18ms
2026-07-02T10:52:55.740174Z TRACE Compiler::run:Compilation::build_chunk_graph: unpack_core::compilation: close time.busy=184µs time.idle=4.46µs
2026-07-02T10:52:55.741663Z TRACE Compiler::run:Compilation::create_assets: unpack_core::compilation: close time.busy=1.46ms time.idle=3.92µs
2026-07-02T10:52:55.741697Z TRACE Compiler::run: unpack_core::compiler: close time.busy=8.40ms time.idle=9.02ms
2026-07-02T10:52:55.742432Z TRACE unpack_node::emit_assets: unpack_node: close time.busy=709µs time.idle=4.75µs
2026-07-02T10:52:55.747219Z TRACE Compiler::flush_cache: unpack_core::compiler: close time.busy=3.64ms time.idle=16.6µs
[unpack tracing] fixture=small phase=warm persistent_cache=on cache_readonly=on filter=unpack_core=trace,unpack_node=trace
2026-07-02T10:52:55.762452Z TRACE Compiler::run:Compilation::make: unpack_core::compilation: close time.busy=3.90ms time.idle=1.96ms
2026-07-02T10:52:55.763750Z TRACE Compiler::run: unpack_core::compiler: close time.busy=5.32ms time.idle=1.90ms
`);

  assert.equal(rows.length, 2);
  assert.equal(rows[0].fixture, "small");
  assert.equal(rows[0].build, "cold");
  assert.equal(rows[0].compilerRun.toFixed(3), "17.420");
  assert.equal(rows[0].make.toFixed(3), "15.050");
  assert.equal(rows[0].chunkGraph.toFixed(3), "0.188");
  assert.equal(rows[0].createAssets.toFixed(3), "1.464");
  assert.equal(rows[0].emitAssets.toFixed(3), "0.714");
  assert.equal(rows[0].flushCache.toFixed(3), "3.657");
  assert.equal(rows[1].fixture, "small");
  assert.equal(rows[1].build, "warm");
  assert.equal(rows[1].compilerRun.toFixed(3), "7.220");
  assert.equal(rows[1].make.toFixed(3), "5.860");

  const markdown = toUnpackTracingSummaryMarkdown(rows);
  assert.match(markdown, /\\| fixture \\| build \\| compiler run ms \\| make ms \\|/);
  assert.match(markdown, /\\| small \\| cold \\| 17\\.420 \\| 15\\.050 \\| 0\\.188 \\| 1\\.464 \\| 0\\.714 \\| 3\\.657 \\|/);
  assert.match(markdown, /\\| small \\| warm \\| 7\\.220 \\| 5\\.860 \\|  \\|  \\|  \\|  \\|/);
});

test("tracing summary can filter rows to one fixture", () => {
  const rows = [
    { fixture: "small", build: "cold", compilerRun: 1 },
    { fixture: "large", build: "cold", compilerRun: 2 },
    { fixture: "large", build: "warm", compilerRun: 3 }
  ];

  const filtered = filterUnpackTracingSummaryRows(rows, { fixture: "large" });
  assert.deepEqual(filtered, [
    { fixture: "large", build: "cold", compilerRun: 2 },
    { fixture: "large", build: "warm", compilerRun: 3 }
  ]);

  const markdown = toUnpackTracingSummaryMarkdown(filtered, { fixture: "large" });
  assert.doesNotMatch(markdown, /small/);
  assert.match(markdown, /\\| large \\| cold \\| 2\\.000 \\|/);
  assert.equal(
    toUnpackTracingSummaryMarkdown([], { fixture: "large" }),
    "No Unpack tracing phase timings were captured for fixture `large`.\n"
  );
});
