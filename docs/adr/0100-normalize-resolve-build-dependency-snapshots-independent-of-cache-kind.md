# Normalize resolve build dependency snapshots independent of cache kind

Unpack will accept and normalize `snapshot.resolveBuildDependencies` regardless of whether the current build cache is disabled, memory-backed, or filesystem-backed, matching webpack's option model. The first effective consumer is filesystem persistent-cache container validation, but keeping normalization independent of cache kind lets users switch cache type without rewriting snapshot configuration.
