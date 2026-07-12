# Require confirmation before performance-driven webpack architecture deviations

Performance optimization does not by itself authorize an implementation that
deviates from webpack's architectural responsibilities, boundaries, naming, or
compilation flow. Before implementing such a deviation, the proposal must
identify the webpack-aligned design, describe the exact deviation, provide the
performance evidence or hypothesis motivating it, explain the compatibility
and maintenance consequences, and receive explicit project agreement to
proceed. The accepted deviation and its boundary must then be documented in an
ADR or in the relevant implementation-differences document.

Until that agreement exists, implementations must preserve the webpack-aligned
architecture even when an alternative appears faster. Benchmarks and profiling
may motivate a proposal, but neither anticipated nor measured performance gains
silently override the alignment goal established by ADR 0137. Rust-native data
representations, ownership, and concurrency remain permitted without separate
confirmation when they preserve webpack's architectural responsibilities and
observable functionality.

`docs/implementation/webpack-architecture-deviation-register.md` is the
central index of active deviations, violations, resolved violations, and
reviewed implementation techniques under this decision.
