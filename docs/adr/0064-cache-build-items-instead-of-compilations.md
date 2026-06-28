# Cache build items instead of compilations

Unpack's build cache will store validated cache items, starting with module build records, instead of storing or restoring whole compilations. A compilation remains a single bundling attempt with fresh graph assembly, while the compiler owns reusable memory and persistent cache layers that can serve individual build results by identifier and validation data; this follows webpack's cache shape without committing Unpack to webpack's hook system.
