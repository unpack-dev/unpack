// Organized to match webpack's lib/ChunkGraph.js responsibility.

import { NativeCompilation } from "./binding.js";
import { Chunk, ChunkImpl } from "./Chunk.js";
import { Module, ModuleImpl } from "./Module.js";

export interface ChunkGraph {
  getModuleId(module: Module): string | number | null;
  getModuleChunksIterable(module: Module): Iterable<Chunk>;
  getOrderedModuleChunksIterable(
    module: Module,
    comparator: (left: Chunk, right: Chunk) => number
  ): Iterable<Chunk>;
  getModuleChunks(module: Module): readonly Chunk[];
  getNumberOfModuleChunks(module: Module): number;
  getNumberOfChunkModules(chunk: Chunk): number;
  getChunkModulesIterable(chunk: Chunk): Iterable<Module>;
  getChunkEntryModulesIterable(chunk: Chunk): Iterable<Module>;
  getOrderedChunkModulesIterable(
    chunk: Chunk,
    comparator: (left: Module, right: Module) => number
  ): Iterable<Module>;
  getChunkModules(chunk: Chunk): readonly Module[];
  getOrderedChunkModules(
    chunk: Chunk,
    comparator: (left: Module, right: Module) => number
  ): readonly Module[];
  isModuleInChunk(module: Module, chunk: Chunk): boolean;
}

export class SortableSetView<T> extends Set<T> {
  #lastComparator: ((left: T, right: T) => number) | undefined;

  sortWith(comparator: (left: T, right: T) => number): boolean {
    if (this.size <= 1 || comparator === this.#lastComparator) return false;
    const ordered = [...this].sort(comparator);
    super.clear();
    for (const value of ordered) super.add(value);
    this.#lastComparator = comparator;
    return true;
  }
}

export const EMPTY_CHUNKS: readonly Chunk[] = [];

export const EMPTY_MODULES: readonly Module[] = [];

export const EMPTY_CHUNK_ITERABLE: SortableSetView<Chunk> = new SortableSetView();

export const EMPTY_MODULE_ITERABLE: SortableSetView<Module> = new SortableSetView();

export class ChunkGraphImpl implements ChunkGraph {
  #nativeCompilation: NativeCompilation | undefined;
  readonly #modulesByHandle: ReadonlyMap<number, ModuleImpl>;
  readonly #chunksByHandle: ReadonlyMap<number, ChunkImpl>;
  readonly #moduleIds = new Map<Module, string | number | null>();
  readonly #moduleChunks = new Map<Module, readonly Chunk[]>();
  readonly #chunkModules = new Map<Chunk, readonly Module[]>();
  readonly #moduleChunkIterables = new Map<Module, SortableSetView<Chunk>>();
  readonly #chunkModuleIterables = new Map<Chunk, SortableSetView<Module>>();
  readonly #orderedChunkModules = new Map<
    Chunk,
    Map<(left: Module, right: Module) => number, readonly Module[]>
  >();

  constructor(
    nativeCompilation: NativeCompilation | undefined,
    modulesByHandle: ReadonlyMap<number, ModuleImpl>,
    chunksByHandle: ReadonlyMap<number, ChunkImpl>
  ) {
    this.#nativeCompilation = nativeCompilation;
    this.#modulesByHandle = modulesByHandle;
    this.#chunksByHandle = chunksByHandle;
  }

  updateNativeCompilation(nativeCompilation: NativeCompilation | undefined): void {
    this.#nativeCompilation = nativeCompilation;
    this.#moduleIds.clear();
    this.#moduleChunks.clear();
    this.#chunkModules.clear();
    this.#moduleChunkIterables.clear();
    this.#chunkModuleIterables.clear();
    this.#orderedChunkModules.clear();
  }

  #loadModuleChunks(module: Module): SortableSetView<Chunk> {
    const loaded = this.#moduleChunkIterables.get(module);
    if (loaded) return loaded;
    if (!(module instanceof ModuleImpl)) return EMPTY_CHUNK_ITERABLE;
    const chunks = (
      this.#nativeCompilation?.moduleChunks(module.nativeHandle) ?? []
    ).flatMap((handle) => {
        const chunk = this.#chunksByHandle.get(handle);
        return chunk ? [chunk] : [];
      });
    const iterable = new SortableSetView<Chunk>(chunks);
    this.#moduleChunkIterables.set(module, iterable);
    this.#moduleChunks.set(module, chunks);
    return iterable;
  }

  #loadChunkModules(chunk: Chunk): SortableSetView<Module> {
    const loaded = this.#chunkModuleIterables.get(chunk);
    if (loaded) return loaded;
    if (!(chunk instanceof ChunkImpl)) return EMPTY_MODULE_ITERABLE;
    const modules = (this.#nativeCompilation?.chunkModules(chunk.nativeHandle) ?? [])
      .flatMap((handle) => {
        const module = this.#modulesByHandle.get(handle);
        return module ? [module] : [];
      });
    const iterable = new SortableSetView<Module>(modules);
    this.#chunkModuleIterables.set(chunk, iterable);
    this.#chunkModules.set(chunk, modules);
    return iterable;
  }

  getModuleId(module: Module): string | number | null {
    if (!this.#moduleIds.has(module)) {
      const id = module instanceof ModuleImpl
        ? this.#nativeCompilation?.moduleId(module.nativeHandle) ?? null
        : null;
      this.#moduleIds.set(module, id);
    }
    return this.#moduleIds.get(module) ?? null;
  }

  getModuleChunksIterable(module: Module): Iterable<Chunk> {
    return this.#loadModuleChunks(module);
  }

  getOrderedModuleChunksIterable(
    module: Module,
    comparator: (left: Chunk, right: Chunk) => number
  ): Iterable<Chunk> {
    const chunks = this.#loadModuleChunks(module);
    if (chunks.sortWith(comparator)) {
      this.#moduleChunks.set(module, [...chunks]);
    }
    return chunks;
  }

  getModuleChunks(module: Module): readonly Chunk[] {
    this.#loadModuleChunks(module);
    return this.#moduleChunks.get(module) ?? EMPTY_CHUNKS;
  }

  getNumberOfModuleChunks(module: Module): number {
    return this.getModuleChunks(module).length;
  }

  getNumberOfChunkModules(chunk: Chunk): number {
    return this.getChunkModules(chunk).length;
  }

  getChunkModulesIterable(chunk: Chunk): Iterable<Module> {
    return this.#loadChunkModules(chunk);
  }

  getChunkEntryModulesIterable(chunk: Chunk): Iterable<Module> {
    if (!(chunk instanceof ChunkImpl)) return EMPTY_MODULE_ITERABLE;
    return new SortableSetView(
      (this.#nativeCompilation?.chunkEntryModules(chunk.nativeHandle) ?? []).flatMap(
        (handle) => {
          const module = this.#modulesByHandle.get(handle);
          return module ? [module] : [];
        }
      )
    );
  }

  getOrderedChunkModulesIterable(
    chunk: Chunk,
    comparator: (left: Module, right: Module) => number
  ): Iterable<Module> {
    const modules = this.#loadChunkModules(chunk);
    modules.sortWith(comparator);
    return modules;
  }

  getChunkModules(chunk: Chunk): readonly Module[] {
    this.#loadChunkModules(chunk);
    return this.#chunkModules.get(chunk) ?? EMPTY_MODULES;
  }

  getOrderedChunkModules(
    chunk: Chunk,
    comparator: (left: Module, right: Module) => number
  ): readonly Module[] {
    let orderedByComparator = this.#orderedChunkModules.get(chunk);
    if (!orderedByComparator) {
      orderedByComparator = new Map();
      this.#orderedChunkModules.set(chunk, orderedByComparator);
    }
    let ordered = orderedByComparator.get(comparator);
    if (!ordered) {
      ordered = [...this.getChunkModules(chunk)].sort(comparator);
      orderedByComparator.set(comparator, ordered);
    }
    return ordered;
  }

  isModuleInChunk(module: Module, chunk: Chunk): boolean {
    return this.#loadChunkModules(chunk).has(module);
  }
}
