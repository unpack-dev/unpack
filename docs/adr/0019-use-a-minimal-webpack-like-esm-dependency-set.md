# Use a minimal webpack-like ESM dependency set

Unpack's first ESM parser output will use a small webpack-like dependency set: `EntryDependency`, `HarmonyImportSideEffectDependency`, `HarmonyImportSpecifierDependency`, `HarmonyExportHeaderDependency`, `HarmonyExportSpecifierDependency`, `HarmonyExportExpressionDependency`, `HarmonyExportImportedSpecifierDependency`, `ConstDependency`, and `ImportDependency`. Static imports, exports, default exports, re-exports, and dynamic imports will be represented through these dependency names so make, chunk graph construction, and source-preserving code generation can stay aligned with webpack concepts while keeping the first implementation narrow.

Static imports and re-exports will emit a side-effect dependency separately from specifier dependencies, matching webpack's split between module evaluation and export usage. Presentational dependencies such as `ConstDependency` and `HarmonyExportHeaderDependency` may clear import or export syntax while graph-bearing harmony dependencies preserve the module relationship.

Re-exports will initially support named re-exports and simple star re-exports through `HarmonyExportImportedSpecifierDependency`. The first implementation will not attempt webpack's full ambiguous star export conflict handling, namespace-object interop, export presence modes, or used-export pruning.
