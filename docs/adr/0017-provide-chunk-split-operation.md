# Provide a chunk split operation

Unpack will provide a webpack-like `Chunk::split` operation that inserts a newly split chunk into every chunk group containing the original chunk and records the reverse chunk-to-group membership. Split-chunks support can then move shared modules into the new chunk without reimplementing chunk group rewiring inside the optimization itself.
