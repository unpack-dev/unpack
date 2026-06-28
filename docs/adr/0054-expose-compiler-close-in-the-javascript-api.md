# Expose compiler close in the JavaScript API

The first JavaScript `Compiler` API will expose `close(callback)` as an explicit lifecycle operation. This keeps the public API prepared for native resources, caches, watchers, and future incremental compilation resources while making cleanup an intentional caller-visible step rather than an implementation detail.
