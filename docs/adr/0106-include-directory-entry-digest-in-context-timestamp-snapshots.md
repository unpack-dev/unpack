# Include directory entry digest in context timestamp snapshots

Unpack context snapshots will include a stable digest of directory entries when using timestamp validation, not just the directory modified time. This mirrors webpack's `timestampHash` idea and protects resolver snapshots from missing directory-content changes on filesystems with coarse or unreliable directory mtimes. Hash validation may later add deeper content hashing, but timestamp-mode context snapshots still need a directory-entry digest.
