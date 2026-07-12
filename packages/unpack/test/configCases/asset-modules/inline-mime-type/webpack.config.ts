import type { ConfigCaseOptions } from "../../../config-case.js";

export default {
  module: {
    rules: [{ test: /[.]avif$/, type: "asset/inline" }]
  }
} satisfies ConfigCaseOptions;
