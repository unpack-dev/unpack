# Async chunk load-once case

Ported from webpack 5.108.1
`test/configCases/target/node-dynamic-import`. The fixture is adapted to
Unpack's supported static-string dynamic imports and verifies Promise exports,
emitted assets, and one successful CommonJS payload load through the public
JavaScript API and an isolated Node process.
