# Create lifecycle alignment matrix before fixes

Before changing JavaScript lifecycle behavior, Unpack will produce `docs/implementation/javascript-lifecycle-webpack-alignment.md` as a lifecycle alignment matrix that compares webpack behavior with current Unpack behavior for the relevant API scenarios. The matrix should classify each difference as an alignment gap or documented webpack deviation and identify the comparison tests and fixes needed, so lifecycle implementation work follows an explicit target rather than redefining alignment during code changes.
