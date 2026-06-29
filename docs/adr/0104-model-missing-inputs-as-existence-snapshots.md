# Model missing inputs as existence snapshots

Unpack will model missing filesystem inputs with explicit missing existence snapshots instead of treating absent paths as ordinary file snapshots. Missing dependency validation only needs to know whether the path has appeared or disappeared, while timestamp and hash strategies apply to existing files or directories. This aligns resolver cache validation with webpack's snapshot model and keeps missing candidates from implying file-content validation work.
