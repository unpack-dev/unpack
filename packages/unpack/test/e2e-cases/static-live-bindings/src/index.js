import { value, setValue } from "./state";

export function run() {
  const before = value;
  setValue(7);
  return [before, value];
}
