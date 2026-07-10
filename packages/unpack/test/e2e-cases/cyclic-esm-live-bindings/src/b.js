import { valueA } from "./a";
export { valueA as fromA } from "./a";

export const valueB = "b";

export function readB() {
  return [valueA, valueB];
}
