# Use a callback-only JavaScript run API

The JavaScript API will expose run completion through Node-style callbacks and will not provide a Promise-returning `run` method at the public API boundary. This deliberately favors webpack-familiar invocation semantics over modern dual Promise/callback ergonomics, keeping the first JavaScript API contract narrower and easier to compare with webpack usage.
