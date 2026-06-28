# Support nested async split points

Unpack's code splitting semantics will include nested async split points: a dynamic import reachable from an async chunk must be able to create its own async chunk group instead of being ignored or folded into the parent async chunk. This keeps dynamic import support semantic rather than limited to entry-reachable imports while still avoiding webpack's broader split-chunks, magic-comment, and import-mode feature set.
