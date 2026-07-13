# Remove Make parallelism configuration

Unpack will not expose a finite Make parallelism setting through Rust `CompilerOptions`. Factorize and Build work remains unbounded in both Make scheduling modes.

The setting was an Unpack-specific compiler surface with no JavaScript API counterpart and no equivalent public webpack option. Removing it also removes the Make semaphore and its per-task permit acquisition. This supersedes the finite parallelism portion of ADR 0143; the serial rebuild scheduling decision remains unchanged.
