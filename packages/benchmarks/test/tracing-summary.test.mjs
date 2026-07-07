import assert from "node:assert/strict";
import test from "node:test";

import {
  filterUnpackTracingSummaryRows,
  parseUnpackTracingSummary,
  toUnpackTracingSummaryMarkdown
} from "../src/tracing-summary.mjs";

test("tracing summary groups major phase timings by fixture and build phase", () => {
  const rows = parseUnpackTracingSummary(`[unpack tracing] fixture=large phase=cold persistent_cache=on cache_readonly=off filter=unpack_core=trace,unpack_node=trace
2026-07-02T10:52:55.739797Z TRACE Compiler::run:Compilation::make: unpack_core::compilation: close time.busy=5.87ms time.idle=9.18ms
2026-07-02T10:52:55.740174Z TRACE Compiler::run:Compilation::build_chunk_graph: unpack_core::compilation: close time.busy=184µs time.idle=4.46µs
2026-07-02T10:52:55.741663Z TRACE Compiler::run:Compilation::create_assets: unpack_core::compilation: close time.busy=1.46ms time.idle=3.92µs
2026-07-02T10:52:55.741697Z TRACE Compiler::run: unpack_core::compiler: close time.busy=8.40ms time.idle=9.02ms
2026-07-02T10:52:55.742432Z TRACE unpack_node::emit_assets: unpack_node: close time.busy=709µs time.idle=4.75µs
2026-07-02T10:52:55.747219Z TRACE Compiler::flush_cache: unpack_core::compiler: close time.busy=3.64ms time.idle=16.6µs
[unpack tracing] fixture=large phase=warm persistent_cache=on cache_readonly=on filter=unpack_core=trace,unpack_node=trace
2026-07-02T10:52:55.762452Z TRACE Compiler::run:Compilation::make: unpack_core::compilation: close time.busy=3.90ms time.idle=1.96ms
2026-07-02T10:52:55.763750Z TRACE Compiler::run: unpack_core::compiler: close time.busy=5.32ms time.idle=1.90ms
[webpack tracing] fixture=large phase=cold persistent_cache=on cache_readonly=off
TRACE Webpack::make: webpack: close time.busy=10.125ms time.idle=0ms
TRACE Webpack::run: webpack: close time.busy=15.250ms time.idle=0ms
TRACE Webpack::build_chunk_graph: webpack: close time.busy=1.500ms time.idle=0ms
TRACE Webpack::create_assets: webpack: close time.busy=2.250ms time.idle=0ms
TRACE Webpack::emit_assets: webpack: close time.busy=3.500ms time.idle=0ms
TRACE Webpack::flush_cache: webpack: close time.busy=4.750ms time.idle=0ms
[rspack tracing] fixture=large phase=cold persistent_cache=on cache_readonly=off
TRACE Rspack::make: rspack: close time.busy=8.125ms time.idle=0ms
TRACE Rspack::run: rspack: close time.busy=13.250ms time.idle=0ms
TRACE Rspack::create_assets: rspack: close time.busy=1.750ms time.idle=0ms
TRACE Rspack::emit_assets: rspack: close time.busy=2.500ms time.idle=0ms
TRACE Rspack::flush_cache: rspack: close time.busy=3.750ms time.idle=0ms
`);

  assert.equal(rows.length, 4);
  assert.equal(rows[0].fixture, "large");
  assert.equal(rows[0].bundler, "unpack");
  assert.equal(rows[0].build, "cold");
  assert.equal(rows[0].compilerRun.toFixed(3), "17.420");
  assert.equal(rows[0].make.toFixed(3), "15.050");
  assert.equal(rows[0].chunkGraph.toFixed(3), "0.188");
  assert.equal(rows[0].createAssets.toFixed(3), "1.464");
  assert.equal(rows[0].emitAssets.toFixed(3), "0.714");
  assert.equal(rows[0].flushCache.toFixed(3), "3.657");
  assert.equal(rows[1].fixture, "large");
  assert.equal(rows[1].bundler, "unpack");
  assert.equal(rows[1].build, "warm");
  assert.equal(rows[1].compilerRun.toFixed(3), "7.220");
  assert.equal(rows[1].make.toFixed(3), "5.860");
  assert.equal(rows[2].fixture, "large");
  assert.equal(rows[2].bundler, "webpack");
  assert.equal(rows[2].build, "cold");
  assert.equal(rows[2].compilerRun.toFixed(3), "15.250");
  assert.equal(rows[2].make.toFixed(3), "10.125");
  assert.equal(rows[3].fixture, "large");
  assert.equal(rows[3].bundler, "rspack");
  assert.equal(rows[3].build, "cold");
  assert.equal(rows[3].compilerRun.toFixed(3), "13.250");
  assert.equal(rows[3].make.toFixed(3), "8.125");

  const markdown = toUnpackTracingSummaryMarkdown(rows);
  assert.match(markdown, /\\| fixture \\| bundler \\| build \\| compiler run ms \\| make ms \\|/);
  assert.match(markdown, /\\| large \\| unpack \\| cold \\| 17\\.420 \\| 15\\.050 \\| 0\\.188 \\| 1\\.464 \\| 0\\.714 \\| 3\\.657 \\|/);
  assert.match(markdown, /\\| large \\| unpack \\| warm \\| 7\\.220 \\| 5\\.860 \\|  \\|  \\|  \\|  \\|/);
  assert.match(markdown, /\\| large \\| webpack \\| cold \\| 15\\.250 \\| 10\\.125 \\| 1\\.500 \\| 2\\.250 \\| 3\\.500 \\| 4\\.750 \\|/);
  assert.match(markdown, /\\| large \\| rspack \\| cold \\| 13\\.250 \\| 8\\.125 \\|  \\| 1\\.750 \\| 2\\.500 \\| 3\\.750 \\|/);
});

test("tracing summary can filter rows to one fixture", () => {
  const rows = [
    { fixture: "other", bundler: "unpack", build: "cold", compilerRun: 1 },
    { fixture: "large", bundler: "unpack", build: "cold", compilerRun: 2 },
    { fixture: "large", bundler: "webpack", build: "warm", compilerRun: 3 }
  ];

  const filtered = filterUnpackTracingSummaryRows(rows, { fixture: "large" });
  assert.deepEqual(filtered, [
    { fixture: "large", bundler: "unpack", build: "cold", compilerRun: 2 },
    { fixture: "large", bundler: "webpack", build: "warm", compilerRun: 3 }
  ]);

  const markdown = toUnpackTracingSummaryMarkdown(filtered, { fixture: "large" });
  assert.doesNotMatch(markdown, /other/);
  assert.match(markdown, /\\| large \\| unpack \\| cold \\| 2\\.000 \\|/);
  assert.equal(
    toUnpackTracingSummaryMarkdown([], { fixture: "large" }),
    "No bundler phase timings were captured for fixture `large`.\n"
  );
});
