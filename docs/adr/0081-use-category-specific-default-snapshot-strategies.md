# Use category-specific default snapshot strategies

Unpack will default module resource snapshots and resolve snapshots to timestamp validation, while build-dependency snapshots default to timestamp plus content-hash validation. Module resources and resolution inputs sit on the watch and rebuild hot path, so the default should stay light for local development, while build dependencies change less often and have broader persistent-cache correctness impact; users can enable hashing explicitly for CI or filesystems where timestamps are unreliable.
