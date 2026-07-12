# Use side-effects flags for module pruning

Unpack will expose webpack-shaped `optimization.sideEffects` values (`true`,
`false`, and `"flag"`) and use package `sideEffects: false` metadata together
with used-export information when constructing the chunk graph. Static modules
whose exports are unused may be omitted only when their package declares them
side-effect-free. Disabling the option retains every statically reachable
module.

The first implementation treats `true` and `"flag"` identically and supports
the package-wide boolean form. Webpack's source-level side-effect analysis and
the `sideEffects` glob-array form remain alignment gaps; they must not be
silently interpreted as side-effect-free.
