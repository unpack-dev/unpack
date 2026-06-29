# Back lifecycle matrix with comparison tests

Unpack will fill the JavaScript lifecycle alignment matrix from committed webpack comparison tests rather than relying on one-off exploration scripts as the source of record. Temporary scripts may help discover behavior and design assertions, but matrix conclusions should be reproducible through tests that execute the pinned webpack reference and current Unpack behavior.
