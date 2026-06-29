# Publish the JavaScript package as ESM-only

The first JavaScript package will expose an ESM-only entry point, so users import Unpack with `import unpack from "unpack"` rather than `require("unpack")`. This is a deliberate deviation from webpack's CommonJS package loading shape and should be revisited if public API parity requires CommonJS entry support.
