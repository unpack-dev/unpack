import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

import type { ConfigCaseTest } from "../../../config-case.js";

const source = Buffer.from([0, 1, 2, 250, 255]);

export default {
  async prepare({ fixturePath }) {
    await writeFile(join(fixturePath, "data.bin"), source);
  },
  async validate({ outputFiles, outputPath, requireEntry }) {
    const resource = outputFiles.find((file) => file !== "main.js");

    assert.match(resource ?? "", /^[a-f0-9]+[.]bin$/);
    assert.deepEqual(await readFile(join(outputPath, resource!)), source);
    assert.equal((requireEntry() as { filename: string }).filename, resource);
  }
} satisfies ConfigCaseTest;
