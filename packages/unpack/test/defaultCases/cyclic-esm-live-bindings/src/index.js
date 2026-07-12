import { readA, setA } from "./a";
import { fromA, readB } from "./b";

export function run() {
  const initial = [[fromA, ...readA()], readB()];
  setA("updated");
  return [...initial, readA()];
}
