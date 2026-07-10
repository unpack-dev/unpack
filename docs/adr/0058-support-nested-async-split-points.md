# Support nested async split points

Unpack's code splitting semantics include nested async split points: a dynamic
import reachable from an async chunk creates its own async chunk group when its
target is not already available on the current loading path. Planning uses a
terminating worklist and parent available-module intersections; an available
back edge creates no cyclic group and code generation preserves asynchronous
Promise timing. Global target reuse retains Dependency Block mappings but omits
any redundant parent edge that would close a cycle between already reachable
Async Chunk Groups. Logical runtime-tree adjacency retains those loading edges
for cycle-safe Runtime Requirement aggregation, so breaking material topology
does not hide deeper capabilities from an Entrypoint. This keeps dynamic import
support semantic rather than limited to entry-reachable imports while still
avoiding webpack's broader split-chunks, magic-comment, and import-mode feature
set.
