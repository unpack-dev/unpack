# Start with a minimal watch options subset

The JavaScript `compiler.watch` API will accept a webpack-familiar but intentionally narrow `watchOptions` shape: `aggregateTimeout`, `ignored`, and `poll`. Unknown watch option keys will throw synchronous `TypeError`s, matching the wrapper's strict option normalization style, while `followSymlinks`, `stdin`, and other watcher controls remain out of scope until Unpack has a concrete need to support them.
