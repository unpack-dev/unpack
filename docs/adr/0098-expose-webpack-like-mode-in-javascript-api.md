# Expose webpack-like mode in the JavaScript API

Unpack will add `mode?: "development" | "production" | "none"` to the JavaScript API and use webpack's defaulting rule where omitted mode behaves as production for defaults that depend on mode. The first consumer is snapshot defaulting: module and resolve snapshots use timestamp plus hash in production or omitted mode, and timestamp-only in development or none. Other production or development defaults should be added explicitly as their corresponding features are introduced.
