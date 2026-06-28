# Use init fragments for module code generation

Unpack will include a webpack-like `InitFragment` concept in module code generation. Dependency templates may both mutate a `rspack_sources` replacement source and append ordered init fragments, allowing export binding initialization and module-level runtime setup to be represented separately from direct source replacements.
