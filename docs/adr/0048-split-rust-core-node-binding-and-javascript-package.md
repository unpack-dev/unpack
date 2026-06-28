# Split Rust core, Node binding, and JavaScript package

Unpack will keep the Rust bundler core in `crates/unpack_core`, add the N-API binding crate in `crates/unpack_node`, and place the public ESM TypeScript package in `packages/unpack`. This keeps core compiler behavior, native Node interop, and JavaScript-facing API normalization as separate ownership boundaries while ensuring JavaScript tests exercise the public package rather than the binding crate directly.
