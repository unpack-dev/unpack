import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join } from "node:path";

import type { ConfigCaseTest } from "../../../config-case.js";

export default {
  async validate({ outputPath, requireEntry }) {
    assert.equal((await readFile(join(outputPath, "main.js"), "utf8")).startsWith("//banner;\n"), true);
    requireEntry();
  }
} satisfies ConfigCaseTest;
