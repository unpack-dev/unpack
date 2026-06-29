# Separate internal tracing from user logging

Unpack will keep developer-facing tracing separate from user-facing infrastructure logging. Rust `tracing` spans and events may be added around coarse compiler phase boundaries for Unpack developers without adding JavaScript API options, while infrastructure log events remain a deliberate JavaScript API surface for users. The same compiler operation may emit both signals, but internal span names, fields, and event frequency must not become the contract for user-visible logging.

Tracing must not become a meaningful cost on normal builds. The first implementation will avoid per-module and per-dependency tracing, avoid collecting trace data when no subscriber is installed, and keep expensive field formatting out of hot paths.

The first span set will be limited to `Compiler::run`, `Compilation::make`, `Compilation::build_chunk_graph`, `Compilation::create_assets`, `unpack_node::emit_assets`, and `Compiler::flush_cache`.
