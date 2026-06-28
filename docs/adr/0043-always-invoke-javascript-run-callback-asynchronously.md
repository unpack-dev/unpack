# Always invoke JavaScript run callback asynchronously

The JavaScript `compiler.run(callback)` API will always invoke its callback after an asynchronous boundary, including failures discovered after a compiler has been created. This avoids synchronous-or-asynchronous callback ambiguity for JavaScript callers while still allowing `unpack(options)` to throw synchronously when the compiler cannot be created from invalid top-level arguments.
