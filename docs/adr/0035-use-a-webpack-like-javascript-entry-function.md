# Use a webpack-like JavaScript entry function

The JavaScript package will export a primary `unpack(options, callback?)` function that creates a compiler and returns it, and a provided callback will run the compiler immediately. This mirrors webpack's familiar programmable entry shape without making webpack API compatibility part of the contract.
