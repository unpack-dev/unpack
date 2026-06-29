# Use selective webpack comparison tests

Unpack will use webpack comparison tests for important exposed JavaScript API behavior, including callback timing, `err` and `stats` semantics, option defaults, validation timing, and supported run, watch, cache, snapshot, and stats behavior. Unpack-specific constraints, native lifecycle details, documented deviations such as ESM-only package loading, and exact error message text can stay covered by ordinary project tests instead of forcing every behavior through a webpack comparison fixture.
