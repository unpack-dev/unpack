# Wrap Rust core with a Node native addon

Unpack's JavaScript API will call the Rust core through a Node.js package backed by an N-API native addon. This keeps the public API as real Node objects and callbacks while avoiding a CLI JSON boundary or a WASM runtime constraint, and it preserves the Rust crate API as an internal implementation boundary rather than the product API.
