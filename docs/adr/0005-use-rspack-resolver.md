# Use rspack_resolver for module resolution

Unpack will use `rspack_resolver` for module resolution instead of implementing a custom filesystem resolver. This aligns the make phase with webpack-like resolution behavior through an existing Rust resolver, keeps package, exports, query, fragment, symlink, and extension behavior outside Unpack's own module graph code, and lets Unpack focus its early architecture on factorization, module identity, parsing, and graph construction.
