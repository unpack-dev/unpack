# Implement snapshot alignment from API to validation model

Unpack will implement snapshot alignment in public-contract-first order: normalize JavaScript `mode` and snapshot options, pass the expanded Rust option model across N-API, introduce `File System Info` with aggregate snapshots, migrate cache records and persistent manifests to those snapshots, and then add behavior tests for webpack-aligned defaults and validation. This sequencing keeps the exposed option contract and error behavior explicit before replacing the lower-level snapshot DTOs.
