# Reject concurrent JavaScript compiler runs

The JavaScript `Compiler` will allow only one active `run` per compiler instance. A second `run` started while the first is still active will receive an asynchronous concurrent-run infrastructure error, preserving webpack-like single-compilation semantics while keeping Unpack's callback timing consistently asynchronous.
