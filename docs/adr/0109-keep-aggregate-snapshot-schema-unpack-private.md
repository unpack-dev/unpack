# Keep aggregate snapshot schema Unpack-private

Unpack aggregate snapshots will align with webpack's validation semantics but will not attempt to serialize snapshots in webpack's object or pack-file format. Snapshot records remain part of Unpack's private persistent cache schema, guarded by cache schema versioning as described in ADR 0082. This lets the Rust implementation use native structs and enums while preserving compatibility through explicit Unpack cache migrations rather than webpack binary/object compatibility.
