# Add context snapshots for directory inputs

Unpack will add context snapshots for directory-sensitive filesystem inputs, following webpack's distinction between file, context, and missing snapshot content. Context snapshots are a lower-level filesystem validation concept and do not imply support for `snapshot.contextModule` or context modules. The first use should be resolver and resolve-build-dependency validation where directory contents can affect resolution results.
