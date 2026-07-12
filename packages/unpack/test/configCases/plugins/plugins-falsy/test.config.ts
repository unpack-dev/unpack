import assert from "node:assert/strict";

import type { ConfigCaseTest } from "../../../config-case.js";

export default {
  validate({ requireEntry }) {
    assert.equal((requireEntry() as { default: unknown }).default, "test");
  }
} satisfies ConfigCaseTest;
