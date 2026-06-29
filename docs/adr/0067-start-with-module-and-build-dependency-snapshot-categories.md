# Start with module and build-dependency snapshot categories

Unpack's snapshot strategy model will be category-based. The first implementation made module resources and build dependencies effective categories; resolve snapshots are added once Unpack stores persistent resolver results, while resolve-build-dependency and context-module categories can wait until the corresponding cache items or context modules exist.
