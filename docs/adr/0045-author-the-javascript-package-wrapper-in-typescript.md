# Author the JavaScript package wrapper in TypeScript

The JavaScript package wrapper will be authored in TypeScript and built to ESM JavaScript plus generated declaration files. This keeps the public JavaScript API contract close to the wrapper source, avoids hand-maintained `.d.ts` drift, and leaves the N-API native addon as an implementation detail loaded by the TypeScript wrapper.
