# Config cases

Config cases follow webpack's `test/configCases/<category>/<case>` layout. Each
case keeps compiler options separate from setup and assertions:

- `webpack.config.ts` exports the case-specific `ConfigCaseOptions`.
- `test.config.ts` can prepare the isolated fixture and validates the result.
- all other files are copied into the fixture before compilation.

The harness owns the isolated `context` and `output.path`. It defaults `entry`
to `./index.js` and `sourcemap` to `false`. Cases only need to declare values
that are relevant to the behavior under test.
