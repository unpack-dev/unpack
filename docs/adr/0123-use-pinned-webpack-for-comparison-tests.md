# Use pinned webpack for comparison tests

Unpack will use a repo-managed, pinned webpack dependency as the executable reference for JavaScript lifecycle comparison tests. Local webpack source checkouts may be used to understand why webpack behaves a certain way, but comparison matrices and tests should derive observable behavior from the pinned dependency so results are reproducible and do not depend on a developer-specific checkout path.
