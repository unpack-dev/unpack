# Do not accept plugins in the first JavaScript API

The first JavaScript API will not yet support a plugin API and will reject a `plugins` option instead of silently ignoring it. This keeps Unpack from implying plugin lifecycle support before that lifecycle has been deliberately designed, while leaving webpack's plugin API shape as the reference for future plugin work.
