# Deferred Failed Module

Ported from webpack 5.108.1 `test/cases/runtime/error-handling` and adapted to
Unpack's supported static-string dynamic import seam. The entry remains usable,
while executing the deferred Failed Module exposes its compilation diagnostic.
