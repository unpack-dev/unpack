# Emit assets for completed compilations with errors

The JavaScript run API will emit assets whenever a compiler run completes a compilation, even if `Stats` reports compilation errors. Only infrastructure errors that prevent the run from completing will be passed as callback `err` and block asset emission, preserving webpack-like separation between fatal run failures and compilation diagnostics.
