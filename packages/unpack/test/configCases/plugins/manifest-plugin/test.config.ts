import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join } from "node:path";

import type { ConfigCaseTest } from "../../../config-case.js";

export default {
  async validate({ outputFiles, outputPath, requireEntry }) {
    assert.equal(outputFiles.includes("third.party.js"), true);
    assert.equal(await readFile(join(outputPath, "third.party.js"), "utf8"), "third party");
    requireEntry();
  }
} satisfies ConfigCaseTest;
