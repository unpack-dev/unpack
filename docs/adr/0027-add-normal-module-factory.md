# Add a normal module factory

Unpack will introduce a webpack-like `NormalModuleFactory` for factorizing module dependencies into normal modules instead of keeping request resolution and module creation directly inside make tasks. The first implementation only needs one normal module factory, but the boundary prepares the make phase for loaders, module types, external modules, and context modules without changing the module graph API.
