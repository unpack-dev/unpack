# Do not clean the output path by default

The JavaScript run API will create `output.path` when needed and overwrite emitted assets with matching filenames, but it will not delete existing files in the output path by default. Cleaning output directories is a destructive behavior that should require an explicit future option rather than being part of the first run contract.
