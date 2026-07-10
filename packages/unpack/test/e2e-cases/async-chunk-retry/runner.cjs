const fs = require("fs");
const path = require("path");

const payloadRuntime = `
exports.runtime = function(__webpack_require__) {
  global.__runtimeAttempts = (global.__runtimeAttempts || 0) + 1;
  global.__runtimeSawFactory = __webpack_require__("./src/feature.js").value;
  if(global.__runtimeAttempts === 1) throw new Error("transient payload runtime");
};
`;

module.exports = async function run(entry) {
  const attempt = () => Promise.resolve().then(() => entry.load());
  const chunk = path.join(process.cwd(), "src_feature_js.js");
  const backup = `${chunk}.backup`;
  fs.renameSync(chunk, backup);

  let loadError;
  try {
    await attempt();
  } catch (error) {
    loadError = error;
  }
  if (loadError === undefined) {
    throw new Error("missing chunk load unexpectedly succeeded");
  }

  fs.renameSync(backup, chunk);
  fs.appendFileSync(chunk, payloadRuntime);

  let runtimeError;
  try {
    await attempt();
  } catch (error) {
    runtimeError = error;
  }
  if (runtimeError === undefined) {
    throw new Error("failing payload runtime unexpectedly succeeded");
  }

  const module = await attempt();
  return [
    loadError.code === "MODULE_NOT_FOUND",
    runtimeError.message,
    module.value,
    global.__runtimeAttempts,
    global.__runtimeSawFactory
  ];
};
