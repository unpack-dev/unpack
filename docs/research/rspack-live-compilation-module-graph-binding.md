# Rspack live `Compilation` / `ModuleGraph` binding assessment

## Scope and conclusion

This note checks Rspack commit
[`cd0781da31eaf107b64eaee8a55db4296527dcb5`](https://github.com/web-infra-dev/rspack/commit/cd0781da31eaf107b64eaee8a55db4296527dcb5),
the repository `HEAD` observed on 2026-07-12.

Rspack does not clone its Rust `ModuleGraph` for `finishModules`, put it behind
an `Arc<RwLock<_>>`, or transfer graph ownership to the Node binding. Its JS
binding holds an unsafe, non-owning pointer to the compiler-owned
`Compilation`; each JS graph query follows that pointer to the current Rust
graph. Rspack awaits the JS `finishModules` promise before advancing the Rust
pipeline, so this is a zero-clone, phase-serialized live view during the hook.

That implementation validates the main performance premise behind Unpack's
current ownership handoff: no full graph clone is required merely to service an
awaited hook. It does not, however, provide a memory-safe pattern worth copying.
Unpack should keep the ownership-handoff design and its post-hook JS-owned view.
The main follow-up is to measure and reduce the new eager O(modules +
connections) N-API materialization cost, because Rspack normally fetches graph
data lazily.

## Rust ownership and storage

Rspack's `Compiler` directly owns one `Compilation` as a struct field; there is
no `Arc<Compilation>` in this ownership chain. The `Compilation` contains a
`StealCell<BuildModuleGraphArtifact>`, and that artifact directly owns the
`ModuleGraph`. `Compilation::get_module_graph()` and
`get_module_graph_mut()` return ordinary references into the artifact.

Sources:

- [`Compiler` owns `Compilation`](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_core/src/compiler/mod.rs#L85-L104)
- [`Compilation` owns the build-module-graph artifact](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_core/src/compilation/mod.rs#L303-L310)
- [`BuildModuleGraphArtifact` owns `ModuleGraph`](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_core/src/artifacts/build_module_graph_artifact.rs#L16-L37)
- [ordinary immutable and mutable graph accessors](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_core/src/compilation/mod.rs#L460-L479)

Rspack does use rollback/overlay containers inside `ModuleGraph`, but for
incremental recovery across make/seal mutations. The source explicitly
classifies fields by make-phase and seal-phase mutation and places graph-module
and connection records in overlay maps. This is not the Node-binding mechanism
and does not create a per-JS-hook graph snapshot.

Source:

- [module-graph rollback/overlay classification and storage](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_core/src/module_graph/mod.rs#L100-L152)

## How the N-API binding exposes the graph

The native `JsCompilation` stores `NonNull<Compilation>`. Its `as_ref()` and
`as_mut()` methods unsafely turn that pointer into `'static` Rust references,
with a comment relying on the `Compiler` not being dropped and its field address
not changing. `JsModuleGraph` independently stores the same kind of
`NonNull<Compilation>` and resolves `compilation.get_module_graph()` afresh for
every call. It rejects access only while the build-module-graph artifact has
been temporarily stolen.

Sources:

- [`JsCompilation` raw pointer and unsafe `'static` references](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_binding_api/src/compilation/mod.rs#L44-L67)
- [`JsCompilation.moduleGraph` constructs a pointer-backed `JsModuleGraph`](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_binding_api/src/compilation/mod.rs#L751-L760)
- [`JsModuleGraph` pointer storage and per-call graph lookup](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_binding_api/src/module_graph.rs#L14-L39)
- [example live connection query](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_binding_api/src/module_graph.rs#L181-L203)

The TypeScript `Compilation` constructs its `moduleGraph` once from that native
binding. `ModuleGraph` methods are thin delegates to native methods; they do not
hold a materialized JS copy of the full graph. The `Compilation.modules` getter
also requests the native module list each time and then wraps it in a new JS
`Set`.

Sources:

- [TypeScript `Compilation` retains the native-backed graph wrapper](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/packages/rspack/src/Compilation.ts#L450-L465)
- [live `modules` getter](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/packages/rspack/src/Compilation.ts#L521-L526)
- [`ModuleGraph` delegates queries to the binding](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/packages/rspack/src/ModuleGraph.ts#L6-L70)

There is therefore no graph clone or graph-level lock on this JS query path.
The binding cost is paid by the particular query: module lists and connection
arrays are converted to JS values as requested.

## `finishModules` timing

Rspack's pass order is build-module-graph, `finishModules`, then seal and the
later optimization/chunk/code-generation passes. The `finishModules` source
describes the topology (modules, connections, dependencies) as frozen at this
boundary, then awaits all hook taps before continuing.

Sources:

- [pass order](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_core/src/compilation/run_passes.rs#L17-L50)
- [`finishModules` boundary and awaited hook call](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_core/src/compilation/finish_modules/mod.rs#L111-L129)

The Node adapter creates a pointer wrapper for the existing `&Compilation` and
awaits `call_with_promise`. On the TypeScript side, the adapter invokes the
existing current `Compilation` object's `finishModules` hook and supplies its
live `modules` getter. No graph or compilation snapshot is passed through this
hook seam.

Sources:

- [binding tap awaits the JS promise with a pointer wrapper](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_binding_api/src/plugins/interceptor.rs#L1298-L1325)
- [TypeScript finish-modules adapter](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/packages/rspack/src/taps/compilation.ts#L303-L317)

Consequently, an async `finishModules` tap can query the live graph for its
whole awaited duration while the Rust pass runner is paused. This is the useful
part of Rspack's design for Unpack to mirror.

## Retained `Compilation` behavior and safety boundary

Within one compilation, Rspack preserves JS object identity. A thread-local
weak-reference map keyed by `CompilationId` returns the same native
`JsCompilation` object each time Rust crosses into JS. TypeScript likewise
stores the current `Compilation`, and later hooks such as `done` receive that
same TypeScript object. Because core mutates the same compiler-owned
`Compilation` throughout all passes, a retained JS object queries later live
seal/done state rather than a `finishModules` snapshot.

Sources:

- [native per-compilation wrapper identity cache](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_binding_api/src/compilation/mod.rs#L1025-L1077)
- [core runs every pass against the same compiler field](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_core/src/compiler/mod.rs#L297-L326)
- [TypeScript creates/caches the current `Compilation`](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/packages/rspack/src/Compiler.ts#L882-L897)
- [`done` receives the compilation returned by that build](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/packages/rspack/src/Compiler.ts#L577-L614)

The tradeoff is explicit unsafe lifetime management, not ownership. The binding
transmutes a mutable compiler reference to `'static`, keeps the JS compiler
alive with a N-API `Reference`, and rejects a second concurrent compiler run.
Its own safety comment says access beyond the compiler state guard can create a
race. There is no phase token, compilation-id check, `RwLock`, or borrow guard
on ordinary `JsCompilation` / `JsModuleGraph` reads.

Source:

- [binding run guard, unsafe lifetime extension, and concurrent-run check](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_binding_api/src/lib.rs#L485-L520)

This means Rspack's normal plugin discipline is part of the safety model:
query during the hook Rust is awaiting. If a plugin retains the object, lets an
async hook resolve, and later queries it from unrelated scheduled JS work while
Rust is advancing another async pass, the raw-pointer API itself does not
serialize that read with Rust mutation. This is an inference from the cited
pointer and pass-runner code, not a documented Rspack guarantee or a claim that
a known race is reproducible in every runtime configuration.

Rebuilds create a new `Compilation` and use `mem::replace` on the compiler's
field, while the binding removes only its weak identity-cache entry. Since
`JsCompilation::as_ref()` does not validate its stored `CompilationId`, an old
user-retained wrapper is not an immutable old-compilation snapshot. At minimum,
cross-rebuild retention is outside the semantics established by this binding;
from the source layout, its pointer continues to address the compiler field
that now contains the next compilation. This is also a source-based inference.

Sources:

- [rebuild replaces the compiler-owned compilation in place](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_core/src/compiler/rebuild.rs#L95-L115)
- [binding cleanup removes wrapper caches](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_binding_api/src/lib.rs#L519-L530)
- [`as_ref()` dereferences without an ID/phase check](https://github.com/web-infra-dev/rspack/blob/cd0781da31eaf107b64eaee8a55db4296527dcb5/crates/rspack_binding_api/src/compilation/mod.rs#L51-L67)

## Implications for Unpack's ownership handoff

The uncommitted Unpack design is more conservative than Rspack in the right
place:

1. Rust moves the `ModuleGraph` into a phase-scoped native lease before calling
   JS. The compiler cannot access the graph while JS owns it, so the awaited
   hook is zero-clone and safe without a lock.
2. A `finally` path returns the graph before seal continues, and native `Drop`
   is a fallback return path. Unlike Rspack, no raw pointer is exposed as a
   `'static` graph reference.
3. Before releasing the native lease, the TypeScript wrapper materializes the
   modules and connections it needs to remain queryable during seal. Later
   `done` rebinds the final native graph and refreshes phase-dependent data on
   the same JS object. This gives retained-object semantics intentionally,
   rather than as an unchecked consequence of a compiler-field pointer.

The cost profile differs:

- Rspack: O(1) to expose the handle; graph records cross N-API lazily per JS
  query; unsafe lifetime/concurrency assumptions remain.
- Current Unpack handoff: O(1) Rust graph move, but O(modules + connections) to
  create/update the persistent JS view at `finishModules`, plus another
  synchronization at final rebinding; no deep Rust graph clone and no live raw
  pointer.

For this repository, keep the Unpack design. Do not replace it with Rspack's
`NonNull<Compilation>` approach merely to match Rspack. The safer ownership
boundary is worth the explicit code, and it better supports the repository's
tested requirement that a retained compilation remain usable across later
phases.

Before considering the clone problem closed, benchmark these separately on a
large graph:

1. the old Rust `ModuleGraph::clone()` time and peak memory;
2. the new `connections()` DTO allocation and N-API conversion time;
3. JS `ModuleGraphConnectionImpl` allocation/indexing time;
4. the second full connection synchronization at final rebinding.

If eager JS materialization is still expensive, optimize the representation
(batched compact arrays, fewer strings, stable handles, and in-place target
patches) rather than reintroducing a raw live pointer. The semantic requirement
to answer arbitrary graph queries while the native lease has returned means
the retained JS view must own enough information; that O(graph size) data
movement can be compressed and batched, but it cannot be made O(1) without
weakening retained-query behavior or introducing another shared snapshot.
