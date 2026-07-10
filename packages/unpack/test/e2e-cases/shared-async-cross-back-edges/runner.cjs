const path = require("path");

module.exports = async function run() {
  require(path.join(process.cwd(), "entry-b.js"));
  await globalThis.loadDirectA();
  const nestedB = await globalThis.loadNestedB();
  let synchronous = true;
  const backA = nestedB.loadA().then(() => [globalThis.aValue, !synchronous]);
  synchronous = false;
  const [backValue, wasAsynchronous] = await backA;
  const directB = await globalThis.loadDirectB();
  let reverseSynchronous = true;
  const nestedA = directB
    .loadA()
    .then(() => [globalThis.aValue, !reverseSynchronous]);
  reverseSynchronous = false;
  const [reverseValue, reverseWasAsynchronous] = await nestedA;
  return [
    globalThis.aValue,
    nestedB.value,
    backValue,
    directB.value,
    reverseValue,
    wasAsynchronous,
    reverseWasAsynchronous
  ];
};
