# cyclic-esm-live-bindings

Ported from webpack 5.108.1 cyclic Harmony module coverage:

- `test/cases/parsing/harmony-cycle`
- `test/cases/parsing/harmony-cycle-reexport` (the named re-export is kept in
  `b.js` so the fixture covers both cycle forms)

The fixture keeps the public JavaScript API execution path isolated from webpack
and checks that export getters are installed before the cycle's imports run.
