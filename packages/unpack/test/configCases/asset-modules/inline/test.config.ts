import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";

import type { ConfigCaseTest } from "../../../config-case.js";

export default {
  async prepare({ fixturePath }) {
    await writeFile(join(fixturePath, "data.bin"), Buffer.from([0, 1, 2, 250, 255]));
  },
  async validate({ outputFiles, requireEntry }) {
    assert.deepEqual(outputFiles, ["main.js"]);
    assert.equal(
      (requireEntry() as { value: string }).value,
      "data:application/octet-stream;base64,AAEC+v8="
    );
  }
} satisfies ConfigCaseTest;
