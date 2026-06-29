# Use webpack-shaped runtime code generation

Unpack will generate runnable JavaScript with webpack-shaped module functions and runtime helpers instead of wrapping raw ESM source directly. Static ESM dependencies will be rewritten to runtime module requests, exports will be represented through generated export bindings, and dynamic imports will become async chunk-loading calls; this keeps output behavior close to webpack while leaving public API shape as a separate product decision.
