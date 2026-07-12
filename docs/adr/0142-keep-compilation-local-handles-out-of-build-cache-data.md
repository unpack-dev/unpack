# Keep compilation-local handles out of Build Cache data

Build Cache data must not use Compilation-local Graph Handles or other
allocation-order-derived identifiers as Cache Identifiers, Cache ETag inputs,
or persisted Cache Item references. This applies to both Memory Cache and
Persistent Cache because both can reuse Cache Items across fresh Compilations.
Cache identity and validation must instead use stable domain identities and the
actual stable inputs that determine the reusable result.

Graph Handles may still index Compilation-owned storage and locate current
Module Graph, Chunk Graph, or Code Generation results while a cache input is
being computed. Values reached through those lookups may participate when they
are stable inputs to the cached result, but the Graph Handle itself must not.
When a reusable computation cannot be expressed without a Compilation-local
identifier, it must remain Compilation-local or use a Compiler-owned memo that
explicitly rebinds stable identities to current handles.

Asset Render Cache ETags therefore include Module Render IDs, rendered module
sources, Runtime Requirements, Runtime Modules, and chunk or entry Render IDs,
but not `ModuleHandle`. This extends ADR 0064's fresh Compilation boundary and
ADR 0131's stable Cache Item identity model without changing the Rust-native
dense-handle representation permitted by ADR 0113.
