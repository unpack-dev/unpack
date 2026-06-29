# Record immutable paths in snapshots

Unpack will record inputs matched by `snapshot.immutablePaths` in snapshot data instead of dropping them entirely. Immutable classification changes validation semantics, but the classified input should remain visible to snapshot inspection, merge behavior, and diagnostics. This follows webpack's model where managed and immutable inputs are represented in snapshot structures while allowing validation to avoid per-file timestamp or hash work for immutable paths.
