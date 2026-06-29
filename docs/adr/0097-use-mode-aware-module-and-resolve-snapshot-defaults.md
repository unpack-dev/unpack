# Use mode-aware module and resolve snapshot defaults

Unpack will align module and resolve snapshot defaults with webpack's mode-aware behavior: production mode, including omitted mode once `mode` is exposed, defaults module and resolve snapshots to timestamp plus hash, while development and none default them to timestamp validation. Build-dependency and resolve-build-dependency snapshots continue to default to timestamp plus hash. This supersedes ADR 0081's fixed timestamp default for module resources and resolution inputs.
