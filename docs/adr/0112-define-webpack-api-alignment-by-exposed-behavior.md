# Define webpack API alignment by exposed behavior

Unpack will measure webpack public API alignment at the behavior level for each webpack-shaped surface it exposes: call shape, option names, defaults, validation and error timing, callback semantics, and main observable behavior should match webpack where practical. Unsupported webpack options, hooks, and feature surfaces should fail loudly or be documented as alignment gaps until intentionally implemented; byte-for-byte output matching and wholesale webpack test-suite parity are not required unless a specific feature chooses them.
