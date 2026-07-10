# Use webpack observations for persistent cache alignment

For Persistent Cache behavior, Unpack will treat observations from the pinned `webpack@5.108.1` executable as normative when webpack documentation disagrees, using the pinned source to explain those observations. This partially supersedes ADR 0114 and ADR 0115 specifically for the observed inert `cache.hashAlgorithm`, `cache.managedPaths`, and `cache.immutablePaths` entries; any other unsupported or inert surface still requires an explicit decision and otherwise fails loudly.
