# Local changes

This vendored copy differs from upstream in the following ways:

- The upstream benchmark and `sst_inspect` binary are omitted. The Criterion
  and `turbo-tasks-malloc` development dependencies used only by those targets
  are omitted as well. Upstream tests and their `rand`, `rayon`, and `tempfile`
  dependencies remain intact.
- `Cargo.toml` is self-contained: workspace-inherited dependencies are replaced
  with explicit upstream version requirements, and publication and automatic
  binary discovery are disabled. Unpack excludes the directory from its
  workspace and consumes it as a path dependency.
- Nightly-only standard-library APIs are replaced for stable Rust compatibility:
  `once_cell::sync::OnceCell` supplies fallible one-time initialization, and a
  small local `SyncUnsafeCell` wrapper preserves the upstream thread-local
  interior-mutability contract.
- Read-only database opening uses `open_directory(true)`, so inspecting an
  existing cache does not perform startup cleanup or create files on disk.

Keep this file current when carrying additional changes across upstream
updates.
