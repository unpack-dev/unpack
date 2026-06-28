# Test webpack-shaped output by structure and semantics

Unpack will verify webpack-shaped output with structural and runtime-semantic tests rather than byte-for-byte webpack snapshots. Fixtures should assert the presence of key runtime shapes such as `__webpack_require__.d`, `__webpack_require__.r`, module render ids, and the chunk loading global, and they should execute generated assets to validate exports, live bindings, re-exports, and dynamic imports.
