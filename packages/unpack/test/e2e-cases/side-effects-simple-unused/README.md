# Side effects simple unused

Ported from webpack 5.108.1's
`test/cases/optimize/side-effects-simple-unused` case. The unused `a.js`
re-export branch is omitted when the package declares `sideEffects: false`.
