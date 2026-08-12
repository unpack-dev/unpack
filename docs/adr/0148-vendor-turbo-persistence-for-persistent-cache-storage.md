# Vendor turbo-persistence for Persistent Cache storage

Unpack will directly vendor `turbopack/crates/turbo-persistence` from
`vercel/next.js` commit `cb36e1d5946eb3bf6473b535e9537a54f257ba27` and use it
for the durable key-value, index, transaction, and compaction work below the
Persistent Cache layer. This removes the need to maintain a second disk storage
engine while retaining Unpack's webpack-shaped cache semantics and atomic batch
publication. The vendored source remains traceable to that revision; necessary
stable-Rust compatibility changes must stay narrow and be documented beside the
vendor.

The Cache, Cache Facade, Cache Layers, four Cache Item DTO and codec families,
Persistent Cache Container guard, idle publication, and best-effort
single-writer contract remain Unpack-owned. The adapter stores its database at
`cacheLocation/turbo-persistence`; its manifest tracks Cache ETags, stable type
and codec identities, and access-aging state used for `maxAge`. Each publication
commits the container guard, manifest, record updates, and deletions in one
turbo-persistence write transaction, whose `CURRENT` publication makes that
state visible atomically. Configured gzip or Brotli compression remains in the
Unpack record envelope before the value is handed to turbo-persistence. A
missing or invalid committed manifest is treated as cold; a writable cache
replaces only its dedicated `turbo-persistence` directory on the next
publication, while a read-only cache never repairs or creates storage.

The previous Unpack-private `index.pack` and content-pack format has no
migration, legacy reader, dual read, or dual write: an existing cache in that
format is treated as cold and is left untouched. This supersedes ADR 0132's
lower storage choice and amends ADR 0131 without changing its webpack-shaped
Cache responsibilities.
