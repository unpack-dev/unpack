// Organized to match webpack's lib/ChunkGroup.js responsibility.

import { Chunk } from "./Chunk.js";

export interface ChunkGroup {
  readonly chunks: readonly Chunk[];
  getFiles(): string[];
}

export class ChunkGroupImpl implements ChunkGroup {
  readonly #files: readonly string[];

  constructor(readonly chunks: readonly Chunk[], files: readonly string[]) {
    this.#files = files;
  }

  getFiles(): string[] {
    return [...this.#files];
  }
}
