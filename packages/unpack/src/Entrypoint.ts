// Organized to match webpack's lib/Entrypoint.js responsibility.

import { Chunk } from "./Chunk.js";
import { ChunkGroup, ChunkGroupImpl } from "./ChunkGroup.js";

export interface Entrypoint extends ChunkGroup {
  getRuntimeChunk(): Chunk | null;
}

export class EntrypointImpl extends ChunkGroupImpl implements Entrypoint {
  readonly #runtimeChunk: Chunk | null;

  constructor(chunks: readonly Chunk[], files: readonly string[], runtimeChunk: Chunk | null) {
    super(chunks, files);
    this.#runtimeChunk = runtimeChunk;
  }

  getRuntimeChunk(): Chunk | null {
    return this.#runtimeChunk;
  }
}
