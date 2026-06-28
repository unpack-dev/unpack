# Separate infrastructure errors from compilation errors

The JavaScript callback `err` parameter will represent infrastructure errors that prevent a compiler run from completing, while source, resolution, and module-processing failures from a completed run will be reported through `Stats`. This follows webpack's familiar error split and keeps build diagnostics available even when a compilation contains errors.
