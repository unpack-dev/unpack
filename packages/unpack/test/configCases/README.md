# Config cases

Cases follow webpack's `test/configCases/<category>/<case>` layout. The files in
each directory determine which kind of case the harness runs:

- A **default case** only needs an `index.js`. It uses the harness defaults and
  the generated entry is executed after a successful compilation.
- A **config case** adds `webpack.config.ts`, which exports its case-specific
  `ConfigCaseOptions`.
- Either kind can add `test.config.ts` when it needs fixture preparation or
  custom validation. When present, its `validate` function replaces the default
  entry execution.
- all other files are copied into the fixture before compilation.

The harness owns the isolated `context` and `output.path`. It defaults `entry`
to `./index.js` and `sourcemap` to `false`. Cases only need to declare values
that are relevant to the behavior under test. This means a simple case can put
its assertions directly in `index.js`, while cases that inspect emitted assets
can use `test.config.ts`.
