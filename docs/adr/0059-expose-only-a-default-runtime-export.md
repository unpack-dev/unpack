# Expose only a default runtime export

The ESM JavaScript package will expose `unpack` as its only runtime export. TypeScript declarations may export types such as `Compiler`, `Stats`, `StatsJson`, and `UnpackOptions`, but the runtime API will not expose `Compiler` or `Stats` constructors, named helper functions, or a version export in the first public surface.
