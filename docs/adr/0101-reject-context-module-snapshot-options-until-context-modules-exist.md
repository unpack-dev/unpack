# Reject context module snapshot options until context modules exist

Unpack will continue to reject `snapshot.contextModule` until context modules are implemented. This keeps the JavaScript API from accepting inert webpack-shaped options and preserves the rule that exposed snapshot options must have active behavior in Unpack. When context modules are added, their snapshot category should be introduced with real validation semantics rather than retrofitting a prior no-op option.
