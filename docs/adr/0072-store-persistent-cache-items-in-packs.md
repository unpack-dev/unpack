# Store persistent cache items in packs

Unpack's persistent cache will use a cache directory with container metadata and cache pack files rather than one file per module or one monolithic cache file. The container will carry cache-version, build-dependency snapshot, and cache-item metadata, while pack files hold serialized cache items; this reduces filesystem overhead compared with per-item files while leaving room for incremental pack writes and future compaction.
