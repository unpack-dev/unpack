import type { ConfigCaseOptions } from "../../../config-case.js";

class FailPlugin {
  apply(): void {
    throw new Error("FailedPlugin");
  }
}

const nullValue = null;
const undefinedValue = undefined;
const falseValue = false;
const zeroValue = 0 as const;
const emptyStringValue = "" as const;

export default {
  plugins: [
    undefinedValue && new FailPlugin(),
    nullValue && new FailPlugin(),
    falseValue && new FailPlugin(),
    zeroValue && new FailPlugin(),
    emptyStringValue && new FailPlugin()
  ]
} satisfies ConfigCaseOptions;
