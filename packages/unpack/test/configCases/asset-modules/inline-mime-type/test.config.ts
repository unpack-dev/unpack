import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";

import type { ConfigCaseTest } from "../../../config-case.js";

export default {
  async prepare({ fixturePath }) {
    await writeFile(join(fixturePath, "data.avif"), "x");
  },
  validate({ requireEntry }) {
    assert.equal(
      (requireEntry() as { value: string }).value,
      "data:image/avif;base64,eA=="
    );
  }
} satisfies ConfigCaseTest;
