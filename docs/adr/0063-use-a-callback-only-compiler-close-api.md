# Use a callback-only compiler close API

The JavaScript `Compiler.close(callback)` API will require a callback and will not return a Promise. Passing a missing or non-function callback is a synchronous `TypeError`, keeping compiler lifecycle operations aligned with the callback-only `run` API.
