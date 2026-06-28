# Do not expose a sourcemap option in the first JavaScript API

The first JavaScript API will not expose `devtool`, `sourceMap`, or any other sourcemap configuration. The JavaScript run will emit whatever assets the Rust core produces, including source map assets when present, and sourcemap configurability can be designed later without inheriting webpack's full `devtool` surface up front.
