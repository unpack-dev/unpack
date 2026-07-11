# Prioritize run errors over close errors in callback entry
status: superseded by ADR-0135

When `unpack(options, callback)` automatically closes the compiler after its internal run, the callback error will be the run infrastructure error if one occurred, otherwise the close infrastructure error if close failed. Compilation errors remain on `Stats`, so a completed run with compilation diagnostics still reports `err === null` unless automatic close fails.
