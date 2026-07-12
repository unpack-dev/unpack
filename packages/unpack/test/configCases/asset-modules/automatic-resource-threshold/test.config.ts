import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

import type { ConfigCaseTest } from "../../../config-case.js";

export default {
  async prepare({ fixturePath }) {
    await writeFile(join(fixturePath, "data.bin"), Buffer.alloc(8097, 1));
  },
  async validate({ outputFiles, outputPath, requireEntry }) {
    const resource = (requireEntry() as { value: string }).value;

    assert.equal(outputFiles.length, 2);
    assert.match(resource, /^[a-f0-9]+[.]bin$/);
    assert.equal((await readFile(join(outputPath, resource))).length, 8097);
  }
} satisfies ConfigCaseTest;
