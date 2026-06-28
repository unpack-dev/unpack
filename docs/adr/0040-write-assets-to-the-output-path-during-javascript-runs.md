# Write assets to the output path during JavaScript runs

The JavaScript `compiler.run(callback)` API will write generated assets to `output.path` instead of returning asset source as the primary product surface. This makes the first JavaScript API behave like a real bundler invocation for downstream JavaScript tests while keeping `Stats` focused on reporting emitted assets and diagnostics.
