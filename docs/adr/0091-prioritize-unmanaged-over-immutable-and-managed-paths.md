# Prioritize unmanaged paths over immutable and managed paths

Unpack snapshot path classification will match webpack's precedence: `unmanagedPaths` overrides every managed or immutable assumption, `immutablePaths` applies next, and `managedPaths` applies last. This gives users an escape hatch for editable packages under otherwise managed directories such as `node_modules`, while still allowing explicitly immutable paths to bypass managed item checks when no unmanaged pattern matches.
