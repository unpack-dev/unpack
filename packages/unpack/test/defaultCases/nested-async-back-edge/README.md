# Nested async split and available-module back edge

Ported from webpack 5.108.1
`test/cases/chunks/nested-blocks-with-available-parent-modules` and
`test/cases/chunks/nested-in-empty`. The fixture adapts the nested loading path
to Unpack's static-string imports and adds a B-to-A async back edge. It verifies
finite Chunk Group construction, two emitted Async Assets, both Promise levels,
deeper Harmony Runtime Requirement propagation through pure-script outer
modules, and asynchronous collapse of the already-available target through the
public JavaScript API and an isolated Node process.
