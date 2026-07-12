import assert from "node:assert/strict";

import type { ConfigCaseTest } from "../../../config-case.js";

export default {
  validate({ outputFiles, requireEntry }) {
    assert.equal(outputFiles.includes("dup.txt"), false);
    requireEntry();
  }
} satisfies ConfigCaseTest;
