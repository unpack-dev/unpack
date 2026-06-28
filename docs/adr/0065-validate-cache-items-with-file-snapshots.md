# Validate cache items with file snapshots

Unpack will validate reusable cache items with file snapshots rather than treating watch events as the source of cache truth. Watch events may trigger compilations and mark known inputs as dirty, but memory and persistent cache reuse must still be decided by validation data attached to each cache item so non-watch runs and later process starts follow the same correctness model.
