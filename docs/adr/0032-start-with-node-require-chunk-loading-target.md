# Start with node require chunk loading target

Unpack's first code generation target will be fixed to a webpack-like node target using CommonJS `require` chunk loading. The implementation will not expose a target or chunk-loading configuration surface initially; browser JSONP, import-scripts, and ESM chunk loading can be added later as explicit output targets once the node-shaped runtime, chunk graph, and generated assets are stable.
