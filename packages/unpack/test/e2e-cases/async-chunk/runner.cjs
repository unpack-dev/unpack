const Module = require("module");

module.exports = async function run(entry) {
  const originalLoad = Module._load;
  let chunkLoads = 0;
  Module._load = function countAsyncPayloadLoads(request, parent, isMain) {
    if (request.endsWith("src_feature_js.js")) chunkLoads += 1;
    return originalLoad.call(this, request, parent, isMain);
  };

  try {
    const first = await entry.run();
    const second = await entry.run();
    return [first, second, chunkLoads];
  } finally {
    Module._load = originalLoad;
  }
};
