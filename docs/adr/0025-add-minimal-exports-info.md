# Add minimal exports info

Unpack will introduce a minimal webpack-like `ExportsInfo` model before implementing tree shaking. The first version will record provided exports and treat every export as used, so harmony export dependency templates can ask for used names through a webpack-shaped interface without committing to full used-exports analysis or namespace re-export conflict handling yet.

## Superseded boundary

The original “treat every export as used” boundary was superseded when the
webpack-shaped `optimization.providedExports` and `optimization.usedExports`
options became exposed. `ExportsInfo` now records disabled, named-used, and
all-used states; named import and re-export usage is propagated to dependency
modules, dynamic namespace imports use the all-used state, and unused harmony
export getters are omitted. Star re-exports still use runtime enumeration when
their complete provided-export set cannot be determined, while named usage is
propagated through them.

The analyses follow webpack's plugin phase boundaries: the Rust
`FlagDependencyExportsPlugin` taps `finish_modules`, while
`FlagDependencyUsagePlugin` taps `optimize_dependencies`. The compiler installs
these built-in plugins conditionally through its `compilation` hook, producing
a fresh hook set for every `Compilation`. `Compilation` only invokes the
corresponding hooks after a successful make and at the beginning of seal;
`finish_modules` is an asynchronous series hook and `optimize_dependencies` is
synchronous.
