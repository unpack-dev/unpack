# Configure serial rebuild Make scheduling

Unpack will keep Tokio-spawned background Make tasks as the default for both initial compilations and rebuilds. The explicit `experiments.serialRebuildMake: true` option changes only rebuild scheduling: Factorize and Build futures are inserted directly into the Make Phase `FuturesUnordered` queue rather than each being wrapped in `tokio::spawn`. Initial compilations continue to use Tokio tasks, and the semaphore continues to enforce configured Make parallelism in both modes.

This is a diagnostic performance experiment, not a different compilation model. Factorize, Add, Build, and Process Dependencies responsibilities, task ordering constraints, errors, and observable JavaScript lifecycle behavior remain unchanged. Keeping the mode opt-in allows profiling to compare task-spawn overhead without silently changing the default scheduler behavior or implying that rebuild work is single-threaded.
