# Use pnpm for the JavaScript workspace

Unpack will use a pnpm workspace for JavaScript package management, with the public package under `packages/unpack`. This keeps JavaScript workspace commands and package filtering straightforward without adding a larger monorepo task runner while the Rust workspace remains managed by Cargo.
