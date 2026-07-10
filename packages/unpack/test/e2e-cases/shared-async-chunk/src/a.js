import { shared } from "./shared";

globalThis.loadA = async function loadA() {
  const feature = await import("./feature");
  return ["a", feature.value, shared];
};
