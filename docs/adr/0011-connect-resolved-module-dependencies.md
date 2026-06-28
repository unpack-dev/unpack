# Connect each resolved ModuleDependency

Unpack will follow webpack's dependency graph semantics for module connections. Dependencies with a resource identifier, such as `ModuleDependency`-style records, may be grouped during factorization so equal requests resolve and build one module, but every resolved module dependency will still receive its own module graph connection. `NullDependency` and `ConstDependency` records do not create module graph connections and only participate in dependency-template code generation.
