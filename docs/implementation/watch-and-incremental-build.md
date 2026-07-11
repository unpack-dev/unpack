# Watch and Incremental Build Implementation Plan

This plan implements watch, memory cache, and persistent filesystem cache across the Rust core, native Node binding, and JavaScript wrapper. The design follows webpack's API and lifecycle shape as far as the current implementation slices allow; unsupported surfaces are staged scope, not a permanent product boundary.

## Target Shape

Unpack should keep `Compilation` as a single bundling attempt while making `Compiler` own reusable state:

1. `Compiler` owns a build cache and creates fresh compilations.
2. `Compilation` reports assets, errors, and watch dependency sets.
3. `WatchSession` observes dependency sets and triggers compilations.
4. `Watching` is the JavaScript handle returned by `compiler.watch`.
5. Persistent cache writes are queued and flushed during compiler idle.

## Slice 1: Stateful Compiler Boundary

Replace the native `runCompiler(options)` shape with a native compiler handle that can be reused by JavaScript `run`, `watch`, and `close`.

- The TypeScript wrapper still validates and normalizes public options.
- The native handle owns the Rust `Compiler` lifecycle.
- `run`, `watch`, and `close` enforce the per-compiler conflict rules from ADR-0076.

## Slice 2: Build Cache and Module Build Records

Introduce a build-cache abstraction before implementing watch.

- Start with memory cache as the default cache layer.
- Cache module build records as cache items rather than whole compilations.
- Make the make phase ask the cache before reading and parsing a module.
- Reassemble module graph and chunk graph for each compilation.

## Slice 3: File Snapshots and Build Dependencies

Add file snapshot validation before persistent cache.

- Support category-specific snapshot strategies for module resources and build dependencies.
- Default module snapshots to timestamp validation.
- Default build-dependency snapshots to timestamp plus content-hash validation.
- Surface normalized cache and snapshot options through the native boundary.

## Slice 4: Persistent Cache Packs

Add filesystem cache as an opt-in cache layer.

- Store persistent cache items in cache packs under a cache location.
- Store container metadata for cache version, build-dependency snapshot, and cache item metadata.
- Store container guards and Cache Item metadata in an Unpack-private binary index, with explicit codecs and lazily referenced content packs.
- Queue persistent writes and flush them during compiler idle.
- Close waits for pending cache flushes or reports infrastructure errors.

## Slice 5: Watch Dependency Sets and Watch Session

Make compilations report watch dependency sets and use them to drive watching.

- Report file dependencies from resolved module resources.
- Report missing dependencies for unresolved requests where available.
- Keep context dependencies available for future context module support.
- Replace watcher subscriptions from each completed compilation.
- Coalesce invalidations with `aggregateTimeout`.

## Slice 6: JavaScript Watch API

Expose the JavaScript API after the native and core lifecycle is stateful.

- `compiler.watch(watchOptions, handler)` starts a watch session and performs the initial compilation.
- The returned `Watching` exposes `close(callback)` and `invalidate()`.
- `watchOptions` initially supports `aggregateTimeout`, `ignored`, and `poll`.
- `unpack(options, callback)` performs a single run and leaves the successfully returned Compiler caller-owned and reusable until explicit close.
