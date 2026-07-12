import { valueB } from "./b";

export let valueA = "a";

export function setA(value) {
  valueA = value;
}

export function readA() {
  return [valueA, valueB];
}
