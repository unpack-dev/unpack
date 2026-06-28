# Align Rust run results with JavaScript error semantics

The Rust core run boundary will treat source, resolution, parsing, and unsupported-module-shape problems as compilation errors recorded on `Compilation` rather than as failed compiler runs. Only infrastructure or internal errors that prevent a compilation from completing should make `Compiler::run()` fail, so the JavaScript API can pass those failures as callback `err` while reporting completed-compilation diagnostics through `Stats`.
