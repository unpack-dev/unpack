module.exports = async function run(entry) {
  const r = await entry.loadR();
  const q = await r.loadQ();
  const cFromQ = await q.loadC();
  const yFromQ = await cFromQ.loadY();

  const p = await entry.loadP();
  const cFromP = await p.loadC();
  const yFromP = await cFromP.loadY();
  return [q.value, cFromQ.shared, yFromQ.value, p.value, cFromP.shared, yFromP.value];
};
