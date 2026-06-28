# Keep failed modules in the graph

When module processing reports a compilation error, Unpack will keep the failed module in the module graph and continue chunk graph construction and asset generation where possible. Generated output for a failed module should throw if runtime execution reaches that module, preserving webpack-like completed-compilation behavior while keeping unaffected runtime paths usable.
