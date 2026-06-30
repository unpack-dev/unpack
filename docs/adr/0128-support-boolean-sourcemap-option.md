# Support a boolean sourcemap option

The JavaScript API will expose a narrow `sourcemap?: boolean` option. It defaults to `true`, preserving the existing behavior of emitting source map assets and `sourceMappingURL` comments. When set to `false`, asset creation emits only JavaScript assets and omits source map references. This supersedes ADR 0069's first-API constraint while still avoiding webpack's full `devtool` surface until Unpack has a broader source map mode model.
