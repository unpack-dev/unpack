import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";

import type { ConfigCaseTest } from "../../../config-case.js";

export default {
  async prepare({ fixturePath }) {
    await writeFile(join(fixturePath, "data.bin"), Buffer.alloc(8096, 1));
  },
  validate({ outputFiles, requireEntry }) {
    assert.deepEqual(outputFiles, ["main.js"]);
    assert.match(
      (requireEntry() as { value: string }).value,
      /^data:application\/octet-stream;base64,/
    );
  }
} satisfies ConfigCaseTest;
