# Reuse webpack architecture by default

Unpack will use webpack's bundler architecture as the default reference for compiler, compilation, module graph, chunk graph, dependency, runtime, cache, and snapshot design. Public API compatibility is still an explicit product choice rather than an automatic promise, but internal architecture should stay webpack-aligned unless a different design has a concrete benefit that is recorded near the decision.
