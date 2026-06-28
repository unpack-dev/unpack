# Test the JavaScript API with node:test

The first JavaScript API tests will use Node's built-in `node:test` runner against the built ESM package output. This avoids committing to an additional JavaScript test framework while still letting tests exercise the public TypeScript wrapper, native addon loading, async callback semantics, asset emission, and Stats reporting through the same package entry point users import.
