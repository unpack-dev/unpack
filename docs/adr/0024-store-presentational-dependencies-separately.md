# Store presentational dependencies separately

Unpack will follow webpack by separating dependencies used for graph construction from dependencies that only affect generated presentation. `DependenciesBlock` records will hold normal dependencies and async blocks, while modules will also have presentational dependencies for source-only replacements such as `ConstDependency` and `HarmonyExportHeaderDependency`. Make will only factorize dependencies with resource identifiers, while code generation will apply templates from normal, async-block, and presentational dependencies.
