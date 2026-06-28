# Use a separate chunk graph

Unpack will derive a chunk graph after the make phase instead of storing chunk ownership directly on the module graph. The module graph remains responsible for modules and dependency edges, while the chunk graph owns initial chunks, async chunks, chunk-to-chunk relationships, and module-to-chunk assignment for code generation; this keeps the first code splitting implementation narrow while leaving room for later split-chunks and runtime chunk work.
