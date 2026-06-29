# Keep comparison tests passing while recording gaps

When a lifecycle comparison scenario exposes a difference between webpack and current Unpack behavior, Unpack will first commit passing observation-style tests that record both behaviors and mark the matrix row as an alignment gap. A later fix should change the scenario to a shared alignment assertion after Unpack has been updated, keeping the main branch green while preserving reproducible evidence for each gap.
