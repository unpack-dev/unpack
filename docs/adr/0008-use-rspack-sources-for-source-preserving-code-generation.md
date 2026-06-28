# Use rspack_sources for source-preserving code generation

Unpack will base module code generation on `rspack_sources`, the Rust port of webpack-sources, instead of reprinting whole modules from AST. The parser still provides dependency and binding metadata, but code generation will preserve original source text and apply dependency-template replacements through source abstractions so sourcemaps and future persistent-cache boundaries align more closely with webpack and Rspack.
