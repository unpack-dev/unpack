# Config cases

Config cases follow webpack's `test/configCases/<category>/<case>` layout. Each
case keeps compiler options separate from setup and assertions:

- `webpack.config.ts` exports the case-specific `ConfigCaseOptions`.
- `test.config.ts` can prepare the isolated fixture and validates the result.
- all other files are copied into the fixture before compilation.

The harness defaults `context` to the copied fixture, `entry` to `./index.js`,
`output.path` to the fixture's `dist` directory, and `sourcemap` to `false`.
Cases only need to declare values that are relevant to the behavior under test.
