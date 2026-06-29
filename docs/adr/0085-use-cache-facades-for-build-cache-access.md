# Use cache facades for build cache access

Unpack build-cache access should follow webpack's cache-facade shape instead of growing subsystem-specific methods on `BuildCache`. The compiler-owned build cache remains the shared storage and validation owner, while make, resolution, code generation, and future asset creation work should access cache items through scoped cache facades unless a different design has a concrete advantage.

Each cache family should be a typed facade over the shared store, not a hand-written wrapper that repeats cache mechanics. This keeps `get`/`store`, disabled-cache behavior, dirty tracking, statistics, and persistence plumbing in one place while letting callers express only their key and record types.
