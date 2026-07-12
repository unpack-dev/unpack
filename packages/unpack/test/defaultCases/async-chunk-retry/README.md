# Async chunk retry and install ordering case

Ported from webpack 5.108.1 `test/configCases/web/retry-failed-import` for the
retry contract and `lib/node/RequireChunkLoadingRuntimeModule.js` for Node
payload installation order. The fixture adapts the web retry case to Unpack's
fixed Node target and static-string import, removes the emitted payload for the
first attempt, and adds a transient payload runtime for the second. It proves
through the public JavaScript API and an isolated Node process that load failures
and payload-runtime failures remain retryable, factories exist before payload
runtime execution, and chunk IDs are marked loaded only after runtime success.
