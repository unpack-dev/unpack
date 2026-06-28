# Make file snapshot validation strategy configurable

Unpack file snapshots will support configurable validation strategies rather than hard-coding timestamp-only or hash-only checks. Following webpack's snapshot model, cache item validation should be able to use timestamps, content hashes, or both, with separate policy hooks for different input categories such as module resources, resolution inputs, and build dependencies; this keeps local development fast while allowing persistent cache correctness to be tuned for CI and filesystems where timestamps are unreliable.
