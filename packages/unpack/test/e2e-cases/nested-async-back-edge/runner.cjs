module.exports = async function run() {
  await globalThis.loadA();
  const b = await globalThis.loadB();
  let synchronous = true;
  const back = b.loadA().then(() => [globalThis.aValue, !synchronous]);
  synchronous = false;
  const [backValue, wasAsynchronous] = await back;
  return [globalThis.aValue, b.value, backValue, wasAsynchronous];
};
