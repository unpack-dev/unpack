import type { ConfigCaseOptions } from "../../../config-case.js";

export default {
  module: {
    rules: [{ test: /[.]bin$/, type: "asset/resource" }]
  }
} satisfies ConfigCaseOptions;
