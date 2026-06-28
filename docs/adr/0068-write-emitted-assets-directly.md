# Write emitted assets directly

The first JavaScript run implementation will write emitted assets directly to their final paths under `output.path`. It will not use temporary files, atomic renames, or whole-run rollback semantics, keeping the initial emit implementation simple while treating write failures as infrastructure errors.
