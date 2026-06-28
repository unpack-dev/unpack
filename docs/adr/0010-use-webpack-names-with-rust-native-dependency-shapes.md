# Use webpack names with Rust-native dependency shapes

Unpack will use webpack-aligned dependency names such as `ModuleDependency`, `NullDependency`, `ConstDependency`, `HarmonyImportDependency`, and `DependencyTemplate`, but it will not copy webpack's JavaScript inheritance model directly into Rust. Dependency implementations may use Rust traits, enums, and composition while preserving webpack-like names and responsibility boundaries so source-preserving code generation and persistent-cache design remain easy to compare with webpack and Rspack.
