# Return stats from the JavaScript run callback

The JavaScript run callback will receive a `Stats` object rather than the internal `Compilation`. This preserves the webpack-familiar `(err, stats)` API shape while keeping mutable build-time compilation state behind the Rust core boundary.
