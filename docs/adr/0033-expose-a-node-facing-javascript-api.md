# Expose a Node-facing JavaScript API

Unpack will expose its programmable product API as a Node.js package that wraps the Rust core, rather than treating the Rust crate API as the primary user-facing API. This keeps Unpack webpack-like for JavaScript users while letting the Rust core remain an internal implementation boundary that can evolve around compiler, compilation, and graph primitives.
