import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";

import type { ConfigCaseTest } from "../../../config-case.js";

const source = Buffer.from([0, 1, 2, 250, 255]);

export default {
  async prepare({ fixturePath }) {
    await writeFile(join(fixturePath, "data.bin"), source);
  },
  validate({ outputFiles, requireEntry }) {
    assert.deepEqual(outputFiles, ["main.js"]);
    assert.equal((requireEntry() as { value: string }).value, source.toString("utf8"));
  }
} satisfies ConfigCaseTest;
