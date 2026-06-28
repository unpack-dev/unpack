# Use JSON manifests and cbor4ii cache packs

Unpack's persistent cache will store human-inspectable container and manifest metadata as JSON, while cache pack shards will store cache item DTOs with `cbor4ii` CBOR serialization. The cache format is an Unpack-private schema guarded by magic bytes, schema version, and cache version; using CBOR improves evolvability and debuggability compared with a compact Rust-specific binary format, while the schema version avoids treating `cbor4ii`'s Serde mapping as a cross-implementation compatibility promise.
