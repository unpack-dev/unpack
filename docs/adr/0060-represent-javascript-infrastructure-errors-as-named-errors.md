# Represent JavaScript infrastructure errors as named Errors

The JavaScript API will report infrastructure failures with ordinary `Error` objects whose `name` identifies the failure category, such as `ConcurrentRunError` or `CompilerClosedError`. The package will not export custom error constructors in the first API surface, avoiding a public error inheritance contract while still giving tests and callers a stable error name to inspect.
