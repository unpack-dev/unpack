# Keep the Watching API surface minimal

The JavaScript `Watching` handle will initially expose `close(callback)` and `invalidate()`. Closing a watching handle must stop future file watching, wait for any active compilation to finish, and wait for pending persistent cache flushes; invalidation triggers a rebuild or coalesces into the next rebuild when compilation is already active, matching webpack's core watch lifecycle without exposing extra suspend, resume, or inspection methods before Unpack has a need for them.
