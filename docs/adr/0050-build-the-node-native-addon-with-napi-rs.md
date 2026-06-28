# Build the Node native addon with napi-rs

The Node binding crate will use `napi-rs` to expose the Rust core through N-API. This gives Unpack a mature Rust-to-Node native addon path with async task support while keeping `napi-rs` generated surfaces internal to the binding layer and leaving the public JavaScript API and declaration files owned by the TypeScript wrapper.
