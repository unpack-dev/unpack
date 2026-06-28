# Resolve JavaScript API paths from context

The JavaScript wrapper will default `context` to `process.cwd()`, resolve relative `context` values from `process.cwd()`, default `output.path` to `<context>/dist`, and resolve relative `output.path` values from the normalized context. Entry requests remain request strings rather than absolute paths so the resolver can preserve module request semantics from the configured context and later issuer directories.
