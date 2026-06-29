# Use aggregate snapshot records

Unpack will replace the narrow `FileSnapshot` and `FileSetSnapshot` validation model with aggregate snapshot records created and validated by `File System Info`. A snapshot can contain file, context, missing-existence, managed item, managed file, managed context, managed missing, and immutable path-classified content. Cache items and persistent cache manifests should store these aggregate snapshots as validation records so module, resolve, build-dependency, and resolve-build-dependency cache data share one invalidation model.
