# E2E cases

Each child directory is one bundle execution case. Add source files exactly as
they should appear in the fixture root, plus a `case.json` manifest:

```json
{
  "runtimeExpression": "entry.run()",
  "expected": "result"
}
```

The runner supplies the shared compiler configuration, copies the case directory
to a temporary fixture, executes the emitted entry asset in Node, and compares
the JSON-serialized result with `expected`.

Optional manifest fields:

- `entry`: override the default `./src/index.js` entry.
- `entryAsset`: override the default emitted `main.js` asset to require.
- `expectedErrors`: require completed-compilation Stats errors containing these strings.
- `expectedErrorCount`: require the exact number of completed-compilation errors.
- `expectedAssets`: require the exact emitted Asset names.
