# Align implemented webpack surfaces before new features

Unpack will prioritize webpack alignment work for already implemented surfaces before opening new webpack feature areas such as loader pipelines, `module.rules`, or plugin hooks. Current public APIs, option behavior, stats, watch, cache and snapshot behavior, generated runtime code, ESM dependency handling, chunk graph semantics, and compilation error behavior should be compared against webpack and corrected where practical before the project accepts broader feature-surface growth.
