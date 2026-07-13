# Configure serial rebuild Make scheduling

Unpack will directly poll rebuild Factorize and Build futures in the Make Phase `FuturesUnordered` queue by default instead of wrapping each future in `tokio::spawn`. The `experiments.serialRebuildMake` option controls this scheduling choice: its default is `true`, while an explicit `false` restores Tokio task spawning for rebuilds. Initial compilations continue to use Tokio tasks in either mode.

Make parallelism is unbounded. The former finite parallelism configuration described here was removed by ADR 0144.

This is a scheduling performance experiment, not a different compilation model. Factorize, Add, Build, and Process Dependencies responsibilities, task ordering constraints, errors, and observable JavaScript lifecycle behavior remain unchanged. The name describes direct in-queue rebuild scheduling and does not imply single-threaded execution.
