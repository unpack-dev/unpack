const answer = 21 * 2;

if (answer !== 42) {
  throw new Error(`expected 42, received ${answer}`);
}
