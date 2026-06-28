# Store dependencies in dependencies blocks

Unpack modules will store synchronous dependencies directly on the module and asynchronous dependencies in `AsyncDependenciesBlock` records, following webpack's dependencies-block shape. The make phase still resolves module dependencies from both module dependencies and async blocks, but the async block boundary is preserved for chunk graph construction and dependency-template code generation.
