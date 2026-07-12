import { createRequire } from "node:module";

import type { WebpackPluginInstance } from "@unpack-js/core";
import type { ConfigCaseOptions } from "../../../config-case.js";

interface BundleAnalyzerPluginConstructor {
  new (options?: Record<string, unknown>): WebpackPluginInstance;
}

const require = createRequire(import.meta.url);
const { BundleAnalyzerPlugin } = require("webpack-bundle-analyzer") as {
  BundleAnalyzerPlugin: BundleAnalyzerPluginConstructor;
};

export const analyzerOptions = {
  analyzerMode: "disabled",
  generateStatsFile: true,
  logLevel: "silent",
  statsFilename: "stats.json"
};

export function createAnalyzerPlugin(): WebpackPluginInstance {
  return new BundleAnalyzerPlugin(analyzerOptions);
}

export default {
  plugins: [createAnalyzerPlugin()]
} satisfies ConfigCaseOptions;
