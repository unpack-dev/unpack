# Nested shared target requeue

Adapted from webpack 5.108.1
`test/cases/chunks/nested-blocks-with-available-parent-modules` and
`test/configCases/chunk-graph/rewalk-chunk`. The C target is first discovered
through P, where X is already available, then through Q, where X is absent.
Shrinking C's all-parent intersection must add X to C and rescan X's nested
dynamic import of Y. The public JavaScript API and isolated Node execution start
with the Q path so a stale first-parent payload cannot be masked.
