# Config cases

Config cases follow webpack's `test/configCases/<category>/<case>` layout:

- `webpack.config.ts` exports the case-specific `ConfigCaseOptions`.
- `test.config.ts` is optional and can provide fixture preparation or
  custom validation. When present, its `validate` function replaces the default
  entry execution.
- all other files are copied into the fixture before compilation.

The harness owns the isolated `context` and `output.path`. It defaults `entry`
to `./index.js` and `sourcemap` to `false`. Cases only need to declare values
that are relevant to the behavior under test. Cases that do not need custom
compiler options belong under `test/cases` instead.
