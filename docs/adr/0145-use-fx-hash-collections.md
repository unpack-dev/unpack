# Use Fx hash collections throughout Rust code

Unpack will use `rustc_hash::FxHashMap` and `rustc_hash::FxHashSet` for Rust-owned hash maps and hash sets instead of `std::collections::HashMap` and `std::collections::HashSet`. This applies to production code, tests, and future Rust crates in the workspace.

The compiler creates and queries hash collections throughout graph construction, parsing, caching, code generation, and watch rebuilds. Standardizing these collections on the fast non-cryptographic hasher used by rustc supports Unpack's performance goal and avoids paying for per-instance randomized hashing where Unpack does not require denial-of-service-resistant hash tables. Code must not rely on Fx collection iteration order; output that requires stable ordering must continue to sort explicitly or use an ordered representation.

This decision does not replace collections whose semantics are different. `BTreeMap` and `BTreeSet` remain appropriate when key order is part of the algorithm or output, and concurrent maps such as `DashMap` remain appropriate when shared concurrent access is required. Dense indexed storage, bit sets, and other specialized representations should also remain in use where they better express the owning webpack responsibility.

The workspace Clippy configuration disallows the standard-library hash map and hash set types, and CI runs that lint across all Rust targets. Introducing another hash collection implementation requires a concrete semantic or performance reason and a deliberate update to this decision and its enforcement configuration.
