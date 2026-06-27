# Build a webpack-like bundler, not a webpack-compatible one

Unpack will implement webpack-like bundling functionality without promising compatibility with webpack's configuration, loader, plugin, or compilation APIs. This keeps the initial Rust and JavaScript API surface small while preserving room to make different architecture choices where webpack compatibility would otherwise dominate the design.
