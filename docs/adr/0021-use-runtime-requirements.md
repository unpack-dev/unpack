# Use runtime requirements

Unpack will model runtime helpers with webpack-like runtime requirements instead of always injecting an undifferentiated runtime block. Dependency templates and init fragments can declare required helpers such as `__webpack_require__`, export getter definition, namespace object marking via `__webpack_require__.r`, module cache, and async chunk loading, while asset creation remains free to conservatively include helpers in the first implementation.
