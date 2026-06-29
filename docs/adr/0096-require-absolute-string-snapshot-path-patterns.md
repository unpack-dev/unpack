# Require absolute string snapshot path patterns

Unpack will require string entries in `snapshot.managedPaths`, `snapshot.immutablePaths`, and `snapshot.unmanagedPaths` to be absolute paths. These options classify filesystem locations globally rather than naming project-local inputs, so relative strings would make path matching depend on caller context in a way webpack does not. Generated defaults such as the `node_modules` managed path pattern remain Unpack-owned and do not require users to provide relative path strings.
