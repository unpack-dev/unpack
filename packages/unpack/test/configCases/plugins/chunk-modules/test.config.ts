import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join } from "node:path";

export default {
  async validate({ outputPath, requireEntry }: { outputPath: string; requireEntry(asset?: string): unknown }) {
    requireEntry();
    assert.notEqual((await readFile(join(outputPath, "chunk-modules.txt"), "utf8")).trim(), "");
  }
};
