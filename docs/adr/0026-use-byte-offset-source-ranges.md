# Use byte-offset source ranges

Unpack dependency records will store source ranges as UTF-8 byte offsets with half-open `[start, end)` semantics. This fits Rust source strings and `rspack_sources` replacement operations while allowing dependency templates to adapt from webpack's JavaScript range conventions without leaking inclusive-end indexing into Unpack's Rust data model.
