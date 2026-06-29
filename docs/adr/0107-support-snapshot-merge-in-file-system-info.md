# Support snapshot merge in File System Info

Unpack `File System Info` will support merging snapshots, following webpack's `mergeSnapshots` behavior. A snapshot merge combines each snapshot content category, takes the earliest available start time, unions set-like managed content, and lets later map entries override earlier entries for the same path. Persistent cache container validation should use snapshot merge for build-dependency and resolve-build-dependency snapshots instead of inventing manifest-specific merge logic.
