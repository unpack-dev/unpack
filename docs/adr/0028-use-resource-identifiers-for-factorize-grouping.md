# Use resource identifiers for factorize grouping

Unpack will use webpack-like dependency resource identifiers to group module dependencies before factorization while continuing to use `ModuleIdentity` for resolved module deduplication. Multiple dependencies with the same resource identifier may share one factory result, but each resolved dependency still receives its own module graph connection.
