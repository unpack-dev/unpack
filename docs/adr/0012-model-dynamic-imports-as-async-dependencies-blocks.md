# Model dynamic imports as async dependencies blocks

Unpack will model static-string dynamic imports with an `AsyncDependenciesBlock` containing an `ImportDependency` instead of representing them only as a dynamic-import dependency kind. The make phase will resolve the `ImportDependency` as a module dependency, while the chunk graph will use the enclosing async dependencies block as the split-point input for async chunk creation.
