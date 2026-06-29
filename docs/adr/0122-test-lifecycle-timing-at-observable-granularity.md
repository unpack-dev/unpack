# Test lifecycle timing at observable granularity

Unpack's first JavaScript lifecycle alignment tests will classify callback timing by observable API boundaries: synchronous validation throws, synchronous method return, and asynchronous callback invocation. Tests should not initially require exact microtask, `process.nextTick`, or macrotask ordering unless a specific webpack lifecycle behavior depends on that finer timing.
