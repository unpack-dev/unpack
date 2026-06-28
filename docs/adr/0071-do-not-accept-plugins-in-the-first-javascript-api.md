# Do not accept plugins in the first JavaScript API

The first JavaScript API will not support a plugin API and will reject a `plugins` option instead of silently ignoring it. This keeps Unpack's webpack-like public API from implying webpack-compatible plugin lifecycle support before that lifecycle has been deliberately designed.
