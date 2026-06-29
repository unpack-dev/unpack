# Publish the JavaScript package as ESM-only

The first JavaScript package will expose an ESM-only entry point, so users import Unpack with `import unpack from "unpack"` rather than `require("unpack")`. This keeps the package format modern and narrow while preserving webpack-like API concepts at the product boundary; CommonJS package loading can be added later if it becomes a product requirement.
