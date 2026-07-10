# Snapshot Build Cache Alignment Implementation Plan

This plan aligns Unpack's build-cache snapshot model with webpack's effective architecture. The work expands the JavaScript snapshot option surface, introduces File System Info, and replaces narrow file-only snapshots with aggregate snapshot records that can validate files, directories, missing inputs, managed items, and persistent cache container dependencies.

## Target Shape

Snapshot validation should have one shared model across memory cache, persistent cache, resolver records, and module build records:

1. The JavaScript API exposes webpack-like `mode` and effective snapshot options.
2. `snapshot.*Paths` are the canonical path classification entrypoint.
3. Each compilation creates its own File System Info, seeded from watch timestamp inputs when available.
4. Persistent cache validation may use a separate longer-lived File System Info.
5. Cache items and cache manifests store aggregate snapshots instead of ad hoc file snapshots.
6. Snapshot semantics align with webpack, while the serialized cache schema remains Unpack-private.

## Key Decisions

- Accept effective snapshot options: `module`, `resolve`, `buildDependencies`, `resolveBuildDependencies`, `managedPaths`, `immutablePaths`, and `unmanagedPaths`.
- Continue rejecting `snapshot.contextModule` until context modules exist.
- Add `mode?: "development" | "production" | "none"`; omitted mode behaves like production for mode-aware defaults.
- Default `module` and `resolve` snapshots to timestamp plus hash in production or omitted mode, and timestamp-only in development or none.
- Default `buildDependencies` and `resolveBuildDependencies` to timestamp plus hash.
- Reject snapshot strategies where both `timestamp` and `hash` are false.
- Support string and RegExp snapshot path patterns; string paths must be absolute.
- Normalize JavaScript RegExp values into source and flags, and only accept no flags or `i`.
- Use Rust regex matching for snapshot path patterns; minor JavaScript RegExp semantic differences are accepted.
- Default managed path classification to `node_modules` only; do not add PnP or Yarn defaults.
- Apply path classification in this precedence order: unmanaged, immutable, managed.
- Model managed paths with webpack-like managed items instead of skipping every file under a managed path.
- Represent missing inputs as missing existence snapshots, not file snapshots.
- Add context snapshots for directory inputs, including a directory-entry digest in timestamp mode.
- Support snapshot merge in File System Info for persistent cache container validation.

## Non-Goals

- Do not add context modules or accept `snapshot.contextModule` as a no-op.
- Do not expose `cache.managedPaths` or `cache.immutablePaths` as effective Unpack entries.
- Do not support PnP or Yarn-specific default path classification.
- Keep snapshot serialization private while aligning snapshot validation semantics with webpack.
- Do not add code generation or asset cache items in this work.

## Slice 1: JavaScript API Normalization

Add `mode` and expand `SnapshotOptions` in the TypeScript wrapper.

- Add `mode?: "development" | "production" | "none"` to public options.
- Normalize omitted mode as production for mode-aware defaults.
- Add `resolveBuildDependencies`, `managedPaths`, `immutablePaths`, and `unmanagedPaths`.
- Keep `contextModule` as an unknown option that throws.
- Reject `{ timestamp: false, hash: false }`.
- Require string path patterns to be absolute.
- Normalize RegExp patterns into `{ source, flags }`.
- Reject unsupported RegExp flags.

Done when JavaScript API tests cover defaults, validation errors, and native option payloads.

## Slice 2: Native and Rust Option Model

Carry the expanded option surface across N-API and into `unpack_core`.

- Add a Rust `Mode` model.
- Add snapshot categories for `resolve_build_dependencies`.
- Add snapshot path pattern DTOs for exact absolute paths and Rust regex patterns.
- Compile regex patterns in Rust, including case-insensitive handling for `i`.
- Build default `node_modules` managed path patterns without PnP or Yarn assumptions.
- Keep snapshot option normalization deterministic between JavaScript and Rust.

Done when Rust compiler options can represent every effective snapshot option without using JavaScript-only types.

## Slice 3: File System Info

Introduce File System Info as the shared snapshot infrastructure.

- Create a per-compilation File System Info.
- Seed compilation File System Info from watch-provided file and context timestamp maps when available.
- Let persistent cache backend validation own a separate File System Info.
- Centralize timestamp, hash, context, managed item, missing existence, and snapshot validity caching.
- Move path classification out of individual cache items.

Done when snapshot creation and validation go through File System Info rather than direct `FileSnapshot` reads.

## Slice 4: Aggregate Snapshot Records

Replace file-only snapshots with aggregate snapshot records.

- Add snapshot content for files, contexts, missing existence, managed item info, managed files, managed contexts, managed missing inputs, and immutable-classified inputs.
- Add context snapshots with directory modified time plus stable directory-entry digest in timestamp mode.
- Add missing existence snapshots that only validate absence/presence.
- Add snapshot merge with webpack-like map override and set union behavior.
- Keep the serialized snapshot schema Unpack-private and encode it through bounded explicit PackFile DTO codecs; cache magic, user cache version, type identifiers, and decode rejection guard incompatible data.

Done when module, resolve, build-dependency, and resolve-build-dependency validation can all store the same aggregate snapshot type.

## Slice 5: Managed Path Semantics

Implement webpack-like managed item behavior.

- Recognize package-like managed items under managed paths.
- Support scoped packages and nested `node_modules`.
- Reject hidden managed items.
- Record special managed states such as missing, `node_modules`, grouping folder, and `name@version` from `package.json`.
- Let unmanaged patterns override managed and immutable matches.
- Let immutable matches bypass per-file work while still being represented in snapshots.

Done when managed package metadata changes invalidate cache entries and unmanaged overrides force normal validation.

## Slice 6: Cache Record Migration

Move cache records and manifests onto aggregate snapshots.

- Change `ModuleBuildRecord` to store an aggregate snapshot for module resources.
- Change `ResolveRecord` to store an aggregate snapshot for file, context, and missing resolver inputs.
- Store build-dependency snapshots in the persistent cache manifest as aggregate snapshots.
- Add resolve-build-dependency snapshots at the persistent cache container level.
- Use snapshot merge when accumulating build-dependency and resolve-build-dependency snapshots.
- Preserve cache disabled and memory cache behavior.

Done when filesystem cache restore validates through aggregate snapshots and stale packs are ignored rather than partially restored.

## Slice 7: Tests

Verify the behavior through JavaScript API and integration tests.

- Omitted mode and production default module and resolve snapshots to timestamp plus hash.
- Development and none default module and resolve snapshots to timestamp-only.
- Snapshot path options accept absolute strings and supported RegExp patterns.
- Snapshot path options reject relative strings and unsupported RegExp flags.
- `snapshot.contextModule` is rejected.
- Unvalidated snapshot strategies are rejected.
- Managed package `name@version` changes invalidate cache.
- Unmanaged patterns override managed defaults.
- Missing resolver candidates appearing on disk invalidate resolve cache.
- Context directory candidate changes invalidate resolve cache.

Done when tests fail against the old file-only snapshot model and pass with File System Info plus aggregate snapshots.

## Suggested Issue Slices

1. Expose mode and expanded snapshot options in the JavaScript API.
2. Add Rust snapshot option models and snapshot path pattern matching.
3. Introduce File System Info and aggregate snapshot records.
4. Implement managed, immutable, unmanaged, missing, and context snapshot semantics.
5. Migrate module, resolve, and persistent cache records to aggregate snapshots.
6. Add JavaScript API behavior tests for webpack-aligned snapshot defaults and invalidation.
