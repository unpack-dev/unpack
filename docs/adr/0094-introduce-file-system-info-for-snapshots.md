# Introduce File System Info for snapshots

Unpack will introduce `File System Info` as the shared snapshot infrastructure boundary, following webpack's architecture. `File System Info` owns snapshot creation, snapshot validation, managed/immutable/unmanaged path classification, managed item inspection, and timestamp/hash caching, while cache items continue to store validation data rather than performing filesystem policy decisions themselves. This prevents module, resolve, build-dependency, and resolve-build-dependency snapshots from drifting into separate invalidation semantics.
