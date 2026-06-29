# Use webpack runtime global names

Unpack's generated runtime will use webpack-shaped internal names such as `__webpack_modules__`, `__webpack_module_cache__`, `__webpack_require__`, `__webpack_exports__`, `__webpack_require__.d`, `__webpack_require__.o`, `__webpack_require__.e`, and `__webpack_require__.u`. These names are part of the generated output shape for debugging and snapshot stability; expanding them into a fuller webpack runtime API should be decided feature by feature.
