# Support a bounded plugins option

The JavaScript API accepts webpack-shaped `plugins` entries now that its
model-backed Compiler and Compilation Hooks can support useful integrations.
Object plugins provide an `apply(compiler)` method. Function-style plugins are
called with the Compiler as both `this` and their argument. Plugins are applied
once in configuration order after Compiler construction and before the first
run; `false`, `null`, `undefined`, `0`, and the empty string are ignored.

Plugin validation and application are part of top-level Compiler
initialization. Without a callback, failures throw synchronously. With a
callback, initialization returns `null` and reports the failure asynchronously
without Stats, following the existing top-level initialization contract.

This option opens only the Hooks and Compiler or Compilation façade properties
that Unpack implements. Each community-plugin compatibility case proves only
the behavior it exercises and does not imply compatibility with arbitrary
webpack plugins or unavailable webpack surfaces.

Community-plugin cases must pass the same plugin through `options.plugins` in
both Unpack and pinned webpack when comparing observable integration output.
The option's bounded normalization rules, apply ordering, function-plugin
calling convention, and initialization-error contract remain direct API
contract tests: they define Unpack's supported subset and are not a claim that
the complete webpack plugin loader or every plugin lifecycle edge is exposed.
