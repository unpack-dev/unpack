# Use category-specific default snapshot strategies

Unpack will default module resource snapshots to timestamp validation and build-dependency snapshots to timestamp plus content-hash validation. Module resources sit on the watch and rebuild hot path, so the default should stay light for local development, while build dependencies change less often and have broader persistent-cache correctness impact; users can enable module hashing explicitly for CI or filesystems where timestamps are unreliable.
