# Start with module and build-dependency snapshot categories

Unpack's snapshot strategy model will be category-based, but the first implementation will make only module resources and build dependencies effective categories. Resolver, resolve-build-dependency, and context-module snapshot categories can be added when Unpack has persistent resolver results or context modules to validate, avoiding placeholder behavior while keeping the cache model aligned with webpack's category-based snapshot configuration.
