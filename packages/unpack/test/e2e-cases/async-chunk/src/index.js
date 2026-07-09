import { label } from "./label";

export async function run() {
  const feature = await import("./feature");
  return [label, feature.value, feature.describe("ok")].join(":");
}
