# Use Rust regex for snapshot path patterns

Unpack snapshot path options will accept both string paths and regular-expression patterns, but regular expressions will be compiled and matched by Rust rather than preserving full JavaScript `RegExp` semantics across N-API. This keeps managed, immutable, and unmanaged path classification inside the Rust snapshot implementation while making the public option surface closer to webpack; minor semantic differences from JavaScript regular expressions are accepted until a concrete compatibility issue requires a narrower bridge.
