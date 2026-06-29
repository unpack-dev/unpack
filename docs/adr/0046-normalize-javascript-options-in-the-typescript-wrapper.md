---
status: superseded by ADR-0079
---

# Normalize JavaScript options in the TypeScript wrapper

The TypeScript wrapper will own JavaScript-facing option validation, defaulting, and normalization, then pass a narrow normalized object to the native addon. Top-level option validation failures during `unpack(options, callback?)` are synchronous `TypeError`s, even when a callback is provided, because no compiler can be created; the first wrapper accepts only `context`, `entry`, and `output` top-level options, so unsupported webpack-shaped options fail loudly until each option is intentionally implemented. This keeps user-facing `TypeError` behavior and webpack-like convenience shapes in TypeScript while the N-API layer focuses on converting normalized options into Rust compiler options, running the core, emitting assets, and returning stats data.
