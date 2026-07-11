import { readFile, writeFile } from "node:fs/promises";

import { toSummaryMarkdown } from "./runner.mjs";

const [currentPath, baselinePath, outputPath] = process.argv.slice(2);

if (!currentPath || !baselinePath || !outputPath) {
  process.stderr.write(
    "Usage: node src/compare-summary.mjs <current.json> <main.json> <output.md>\n"
  );
  process.exitCode = 1;
} else {
  const [current, baseline] = await Promise.all([
    readReport(currentPath),
    readReport(baselinePath)
  ]);
  await writeFile(outputPath, toSummaryMarkdown(current, baseline));
}

async function readReport(path) {
  return JSON.parse(await readFile(path, "utf8"));
}
