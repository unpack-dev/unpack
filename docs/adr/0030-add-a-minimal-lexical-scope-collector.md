# Add a minimal lexical scope collector

Unpack will add a minimal lexical scope collector to support import usage rewriting for live bindings instead of depending on a full SWC resolver in the first implementation. The collector will track imported bindings, local declarations, and shadowing across module, function, and block scopes so `HarmonyImportSpecifierDependency` records can carry safe usage ranges for source-preserving code generation.
