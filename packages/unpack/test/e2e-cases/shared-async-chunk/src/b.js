globalThis.loadB = async function loadB() {
  const feature = await import("./feature");
  return ["b", feature.value, feature.shared];
};
