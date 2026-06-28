# Rewrite import usages for live bindings

Unpack will preserve webpack-like ESM live binding semantics by rewriting imported binding usages to read from the imported module object instead of creating local snapshot variables. `HarmonyImportDependency` records provide per-request import variables, `HarmonyImportSpecifierDependency` records the requested export ids and usage ranges, and dependency templates replace those usage ranges with property access on the import variable during source-preserving code generation.

Imported binding writes will also be rewritten rather than rejected during parsing. This matches webpack's behavior: assigning to an imported binding compiles to an assignment against the imported module namespace property, which then fails at runtime when the export is exposed through a getter-only binding.
