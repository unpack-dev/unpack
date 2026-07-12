# Default cases

Default cases follow webpack's `test/cases/<category>/<case>` layout. A simple
case only needs an `index.js`; the harness compiles and executes the generated
entry with its default options.

A case can add `test.config.ts` when it needs fixture preparation or custom
validation. Cases that need custom compiler options belong under
`test/configCases` and provide a `webpack.config.ts` file.
