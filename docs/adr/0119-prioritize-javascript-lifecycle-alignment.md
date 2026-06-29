# Prioritize JavaScript lifecycle alignment

Unpack's first exposed JavaScript API alignment pass will focus on lifecycle behavior for `unpack(options, callback?)`, `compiler.run(callback)`, `compiler.close(callback)`, `compiler.watch(...)`, `Watching.invalidate()`, `Watching.close(callback)`, and `Stats`. This work should compare synchronous validation, asynchronous callback timing, callback `err` versus `stats.hasErrors()` behavior, returned stats availability, and run, watch, close, and watching conflict semantics against webpack before moving to option-specific alignment.
