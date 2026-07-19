# Freeze Modules before the Finished Modules Boundary

Unpack will represent Make-time Module construction with `BuildingModule`
values in a Make-owned `BuildingModuleGraph`. Make consumes those values into
`Module` values in the Compilation's `ModuleGraph`; the resulting `Module`
type exposes no mutation interface.
Consequently all internal and host `finishModules` hooks, sealing phases, and
later lifecycle work can read Module build state but cannot mutate it through
the Rust type system.

`ExportsInfo` is Compilation-specific graph analysis rather than Module build
state. It is indexed by `ModuleHandle` and owned by `ModuleGraph`, matching
webpack's responsibility boundary. `FlagDependencyExportsPlugin` may write its
provided-export state during `finish_modules`, and
`FlagDependencyUsagePlugin` may write its used-export state during
`optimize_dependencies`; these mutations do not mutate `Module` values.
Render IDs, chunk membership, Module Hashes, Code Generation Results, and
assets remain in their existing Compilation-owned phase structures.

The Finished Modules Boundary remains after both the internal and awaited host
`finishModules` work. Modules are currently frozen slightly earlier, when Make
consumes `BuildingModuleGraph`, which gives the boundary the stronger invariant
without removing the webpack-shaped hook timing. A future webpack-compatible
surface that requires post-build Module transformation must model that work as
an explicit transform result or supersede this decision rather than restoring
a general `ModuleGraph::module_mut` escape hatch.

This refines ADR 0025's Exports Info model and ADR 0139's Module Computation
Cache inputs by assigning Exports Info to Module Graph metadata; their phase
ordering and cache semantics remain unchanged.
