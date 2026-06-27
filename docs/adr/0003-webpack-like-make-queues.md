# Use webpack-like make queues

Unpack will structure the make phase around webpack-like factorize, add, build, and process-dependencies stages while implementing their execution with Rust concurrency primitives. This keeps the architecture recognizable for bundler work without copying webpack's JavaScript `AsyncQueue` API, and it gives each stage a clear ownership boundary for resolving requests, deduplicating modules, reading and parsing source, and connecting graph edges.
