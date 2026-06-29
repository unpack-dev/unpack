# Use dedicated lifecycle comparison test file

Unpack will place JavaScript lifecycle comparison tests in `packages/unpack/test/webpack-lifecycle-alignment.test.ts` rather than mixing them into the existing `packages/unpack/test/api.test.ts` regression suite. Keeping comparison tests separate makes it clear which tests execute both webpack and Unpack, which tests inform the lifecycle alignment matrix, and which tests cover ordinary Unpack API behavior.
