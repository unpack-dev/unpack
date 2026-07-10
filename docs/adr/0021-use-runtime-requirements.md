# Use runtime requirements

Unpack models runtime helpers with webpack-like Runtime Requirements instead of
always injecting an undifferentiated runtime block. Dependency Templates, Init
Fragments, and startup declare their direct requirements. The Chunk Graph stores
processed module, chunk, and Entrypoint runtime-tree requirements.

A crate-internal fixed-point resolver adds transitive requirements, selects and
deduplicates Runtime Modules, and orders them by the Normal, Basic, Attach, and
Trigger stages followed by stable identifier. Asset creation renders only the
selected modules. Runtime Requirements and Runtime Modules are closed enums, so
unknown capabilities fail exhaustive compilation and conflicting stable module
identifiers fail explicitly during resolution. The remaining legacy asynchronous Node runtime is retained
only for Entrypoint runtime trees that require chunk ensuring until its dedicated
migration.
