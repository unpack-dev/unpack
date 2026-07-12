# Config cases

Config cases follow webpack's `test/configCases/<category>/<case>` layout:

- `webpack.config.ts` exports the case-specific `ConfigCaseOptions`.
- `test.config.ts` is optional and can provide fixture preparation or
  custom validation. When present, its `validate` function replaces the default
  entry execution.
- all other files are copied into the fixture before compilation.

Cases that document an upstream JavaScript API gap can provide a
`case.config.json` with `skip.issue`, `skip.reason`, and `skip.upstream`. The
harness registers these as skipped tests and includes the tracking issue in the
skip reason. These ported cases can preserve an upstream-style
`webpack.config.js`; after the model-backed surface is implemented, remove the
skip metadata and the harness will load that configuration directly.

The harness owns the isolated `context` and `output.path`. It defaults `entry`
to `./index.js` and `sourcemap` to `false`. Cases only need to declare values
that are relevant to the behavior under test. Cases that do not need custom
compiler options belong under `test/cases` instead.
