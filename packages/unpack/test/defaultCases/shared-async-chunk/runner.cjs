const Module = require("module");
const path = require("path");

module.exports = async function run() {
  const originalLoad = Module._load;
  let chunkLoads = 0;
  Module._load = function countSharedPayloadLoads(request, parent, isMain) {
    if (request.endsWith("src_feature_js.js")) chunkLoads += 1;
    return originalLoad.call(this, request, parent, isMain);
  };

  try {
    require(path.join(process.cwd(), "a.js"));
    require(path.join(process.cwd(), "b.js"));
    const fromA = await globalThis.loadA();
    const fromB = await globalThis.loadB();
    return [fromA, fromB, chunkLoads];
  } finally {
    Module._load = originalLoad;
  }
};
