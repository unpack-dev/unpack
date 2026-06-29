# Normalize snapshot path regex flags

Unpack will normalize JavaScript `RegExp` snapshot path patterns into source and flags before passing them to Rust, and the first implementation will accept only no flags or `i`. Unsupported JavaScript flags are rejected instead of ignored because Rust performs the matching and cannot promise full JavaScript regular-expression behavior; this preserves a clear API contract while still allowing case-insensitive managed, immutable, and unmanaged path patterns.
