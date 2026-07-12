// Organized to match webpack's lib/Chunk.js responsibility.

export interface Chunk {
  readonly id: string | number | null;
  readonly name?: string;
}

export class ChunkImpl implements Chunk {
  constructor(
    readonly nativeHandle: number,
    readonly id: string | number | null,
    readonly name: string | undefined
  ) {}
}
