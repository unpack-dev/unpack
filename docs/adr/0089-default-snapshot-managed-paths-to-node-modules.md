# Default snapshot managed paths to node_modules only

Unpack will default snapshot managed-path classification to `node_modules` and will not add webpack's PnP or Yarn cache defaults in the first implementation. This keeps the default persistent-cache invalidation model aligned with the most common Node package layout while avoiding package-manager-specific assumptions that Unpack does not otherwise support; users can still configure explicit managed, immutable, or unmanaged path patterns when their workspace layout needs different behavior.
