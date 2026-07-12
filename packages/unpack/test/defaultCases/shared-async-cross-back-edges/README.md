# Shared async targets with cross back edges

Adapted from webpack 5.108.1
`test/cases/chunks/nested-blocks-with-available-parent-modules`. Two independent
Entrypoints reach A and B, which dynamically import each other. The fixture
verifies that Unpack retains both globally reusable target mappings without
materializing an A-to-B-to-A Chunk Group cycle, while logical runtime-tree
reachability still propagates B's Harmony-only helpers into the pure-script A
runtime. Both directions execute in isolated Entrypoint runtimes and an
already-installed nested load remains asynchronous.
