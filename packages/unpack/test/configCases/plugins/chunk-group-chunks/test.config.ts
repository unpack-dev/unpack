export default {
  validate({ requireEntry }: { requireEntry(asset?: string): unknown }) {
    requireEntry("a.js");
    requireEntry("b.js");
  }
};
