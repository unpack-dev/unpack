# Use a webpack-like JavaScript entry function

The JavaScript package will export a primary `unpack(options, callback?)` function that creates a compiler and returns it, and a provided callback will run the compiler immediately. Passing a non-function callback is a synchronous `TypeError`. This mirrors webpack's familiar programmable entry shape and makes webpack's public API shape the default reference for future JavaScript API work.

ADR 0135 refines this decision: a Compiler is returned only after successful construction. A callback overload reports construction failures asynchronously and returns no Compiler.
