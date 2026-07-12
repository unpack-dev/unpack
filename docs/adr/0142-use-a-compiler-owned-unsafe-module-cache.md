# Use a compiler-owned unsafe module cache

Unpack will implement webpack's `module.unsafeCache` responsibility with a
Compiler-owned, process-local Unsafe Module Cache. The cache is enabled only
when the ordinary Build Cache is enabled. Omitted `module.unsafeCache` follows
webpack by selecting modules under `node_modules`; explicit `true` selects all
factorized modules and explicit `false` disables reuse. Predicate functions are
rejected until the JavaScript Module predicate can be evaluated without moving
Module ownership out of Rust.

Entries are keyed by the factorize grouping inputs: issuer context, module
factory, dependency category, and dependency resource identifier. Each entry
stores the factorized Module together with the loader and resolve dependency
metadata needed to restore its factory result. A hit is checked before work is
admitted to the factorize queue. The cached Module receives a fresh Graph Handle
in the new Compilation, current dependencies are connected to it, and it still
enters the ordinary Build stage so Module Build Record and Snapshot validation
remain active. Compilations and their Module Graphs therefore remain fresh under
ADR 0064.

The Unsafe Module Cache is distinct from the validated, record-oriented Cache
and Cache Facades in ADRs 0085 and 0131, and from the unaffected computation
memos in ADR 0139. This is a staged Rust representation of webpack's dependency
weak references and `Compilation/modules` cache. Entries are not published to
PackFile, so a new Compiler process falls back to the validated Resolve and
Module Build caches. Persistent unsafe-module restoration and the predicate
function form should be added together with the Module serialization and public
predicate boundary needed to preserve webpack's observable behavior.
