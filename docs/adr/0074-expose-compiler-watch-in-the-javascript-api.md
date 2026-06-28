# Expose compiler watch in the JavaScript API

Unpack will expose `compiler.watch(watchOptions, handler)` in the JavaScript API and return a `Watching` handle for closing and invalidating the watch session. This supersedes the earlier first-API decision to omit watch because the JavaScript wrapper and native boundary now exist, and watch must be a compiler-owned lifecycle so it can share incremental build cache, file snapshot validation, persistent cache idle flush, and close semantics with `run`.
