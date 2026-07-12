# Shared async chunk across entrypoints

Ported from webpack 5.108.1 `test/configCases/chunk-graph/issue-9634` and the
Node dynamic-import behavior in
`test/configCases/target/node-dynamic-import`. The fixture adapts the scenarios
to Unpack's fixed Node target: one parent already has `shared`, the other does
not, both execute the same Async Chunk, and each Entrypoint runtime independently
requests and installs the payload through the public JavaScript API and an
isolated Node process.
