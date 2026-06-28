# Default to memory cache

Unpack will treat omitted `cache` and `cache: true` as memory cache, while persistent filesystem cache requires `cache: { type: "filesystem" }`. This keeps watch and repeated runs on the same compiler fast by default, follows webpack's cache default, and makes cross-process persistent cache an explicit opt-in while its invalidation and serialization behavior matures.
