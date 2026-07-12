// Organized to match webpack's lib/Compilation.js responsibility.

import type { Source } from "webpack-sources";

import { NativeAssetSource, NativeCompilation, RawSource } from "./binding.js";
import { Chunk, ChunkImpl } from "./Chunk.js";
import { ChunkGraph, ChunkGraphImpl } from "./ChunkGraph.js";
import { ChunkGroup, ChunkGroupImpl } from "./ChunkGroup.js";
import { Entrypoint, EntrypointImpl } from "./Entrypoint.js";
import { Module, ModuleImpl } from "./Module.js";
import { ModuleGraph, ModuleGraphImpl } from "./ModuleGraph.js";
import { ModuleGraphConnection } from "./ModuleGraphConnection.js";
import { TapOptions, insertOrderedTap, normalizeTapOptions, assertFunction, assertNonEmptyString, toError } from "./util.js";

export interface Compilation {
  readonly hooks: CompilationHooks;
  readonly moduleGraph: ModuleGraph;
  readonly chunkGraph: ChunkGraph;
  readonly modules: ReadonlySet<Module>;
  readonly chunks: ReadonlySet<Chunk>;
  readonly chunkGroups: readonly ChunkGroup[];
  readonly namedChunkGroups: ReadonlyMap<string, ChunkGroup>;
  readonly entrypoints: ReadonlyMap<string, Entrypoint>;
  readonly assets: Record<string, Source>;
  emitAsset(name: string, source: Source): void;
  updateAsset(name: string, source: Source | ((source: Source) => Source)): void;
  getAsset(name: string): CompilationAsset | undefined;
}

export interface CompilationAsset {
  readonly name: string;
  readonly source: Source;
}

export interface FinishModulesHook {
  tap(options: string | TapOptions, callback: (modules: ReadonlySet<Module>) => void): void;
  tapAsync(
    options: string | TapOptions,
    callback: (modules: ReadonlySet<Module>, done: (error?: Error | null) => void) => void
  ): void;
  tapPromise(
    options: string | TapOptions,
    callback: (modules: ReadonlySet<Module>) => PromiseLike<void>
  ): void;
  callAsync(
    modules: ReadonlySet<Module>,
    callback: (error?: Error | null) => void
  ): void;
  promise(modules: ReadonlySet<Module>): Promise<void>;
}

export interface ProcessAssetsHook {
  tap(
    options: string | TapOptions,
    callback: (assets: Record<string, Source>) => void
  ): void;
  tapAsync(
    options: string | TapOptions,
    callback: (
      assets: Record<string, Source>,
      done: (error?: Error | null) => void
    ) => void
  ): void;
  tapPromise(
    options: string | TapOptions,
    callback: (assets: Record<string, Source>) => PromiseLike<void>
  ): void;
  callAsync(
    assets: Record<string, Source>,
    callback: (error?: Error | null) => void
  ): void;
  promise(assets: Record<string, Source>): Promise<void>;
}

export interface CompilationHooks {
  readonly finishModules: FinishModulesHook;
  readonly processAssets: ProcessAssetsHook;
}

export interface FinishModulesTap {
  name: string;
  stage: number;
  before: Set<string>;
  run(modules: ReadonlySet<Module>): Promise<void>;
}

export class FinishModulesHookImpl implements FinishModulesHook {
  readonly #taps: FinishModulesTap[] = [];

  tap(options: string | TapOptions, callback: (modules: ReadonlySet<Module>) => void): void {
    assertFunction(callback, "callback");
    this.#insert(options, async (modules) => { callback(modules); });
  }

  tapAsync(
    options: string | TapOptions,
    callback: (modules: ReadonlySet<Module>, done: (error?: Error | null) => void) => void
  ): void {
    assertFunction(callback, "callback");
    this.#insert(options, (modules) => new Promise<void>((resolve, reject) => {
      let settled = false;
      const done = (error?: Error | null): void => {
        if (settled) return;
        settled = true;
        error == null ? resolve() : reject(error);
      };
      try { callback(modules, done); } catch (error) { done(toError(error, "HookError")); }
    }));
  }

  tapPromise(
    options: string | TapOptions,
    callback: (modules: ReadonlySet<Module>) => PromiseLike<void>
  ): void {
    assertFunction(callback, "callback");
    this.#insert(options, async (modules) => { await callback(modules); });
  }

  callAsync(
    modules: ReadonlySet<Module>,
    callback: (error?: Error | null) => void
  ): void {
    assertFunction(callback, "callback");
    void this.promise(modules).then(() => callback(), (error) => callback(toError(error, "HookError")));
  }

  async promise(modules: ReadonlySet<Module>): Promise<void> {
    for (const tap of this.#taps) await tap.run(modules);
  }

  #insert(
    options: string | TapOptions,
    run: (modules: ReadonlySet<Module>) => Promise<void>
  ): void {
    const tap = { ...normalizeTapOptions(options), run };
    insertOrderedTap(this.#taps, tap);
  }
}

export interface ProcessAssetsTap {
  name: string;
  stage: number;
  before: Set<string>;
  run(assets: Record<string, Source>): Promise<void>;
}

export class ProcessAssetsHookImpl implements ProcessAssetsHook {
  readonly #taps: ProcessAssetsTap[] = [];

  tap(
    options: string | TapOptions,
    callback: (assets: Record<string, Source>) => void
  ): void {
    assertFunction(callback, "callback");
    this.#insert(options, async (assets) => { callback(assets); });
  }

  tapAsync(
    options: string | TapOptions,
    callback: (
      assets: Record<string, Source>,
      done: (error?: Error | null) => void
    ) => void
  ): void {
    assertFunction(callback, "callback");
    this.#insert(options, (assets) => new Promise<void>((resolve, reject) => {
      let settled = false;
      const done = (error?: Error | null): void => {
        if (settled) return;
        settled = true;
        error == null ? resolve() : reject(error);
      };
      try { callback(assets, done); } catch (error) { done(toError(error, "HookError")); }
    }));
  }

  tapPromise(
    options: string | TapOptions,
    callback: (assets: Record<string, Source>) => PromiseLike<void>
  ): void {
    assertFunction(callback, "callback");
    this.#insert(options, async (assets) => { await callback(assets); });
  }

  callAsync(
    assets: Record<string, Source>,
    callback: (error?: Error | null) => void
  ): void {
    assertFunction(callback, "callback");
    void this.promise(assets).then(
      () => callback(),
      (error) => callback(toError(error, "HookError"))
    );
  }

  async promise(assets: Record<string, Source>): Promise<void> {
    for (const tap of this.#taps) await tap.run(assets);
  }

  isUsed(): boolean {
    return this.#taps.length > 0;
  }

  #insert(
    options: string | TapOptions,
    run: (assets: Record<string, Source>) => Promise<void>
  ): void {
    const tap = { ...normalizeTapOptions(options), run };
    insertOrderedTap(this.#taps, tap);
  }
}

export class CompilationImpl implements Compilation {
  readonly hooks: CompilationHooks = {
    finishModules: new FinishModulesHookImpl(),
    processAssets: new ProcessAssetsHookImpl()
  };
  readonly moduleGraph: ModuleGraphImpl;
  readonly chunkGraph: ChunkGraphImpl;
  readonly modules: ReadonlySet<Module>;
  readonly chunks: ReadonlySet<Chunk>;
  chunkGroups: readonly ChunkGroup[] = [];
  namedChunkGroups: ReadonlyMap<string, ChunkGroup> = new Map();
  entrypoints: ReadonlyMap<string, Entrypoint> = new Map();
  readonly #modulesByHandle = new Map<number, ModuleImpl>();
  readonly #chunksByHandle = new Map<number, ChunkImpl>();
  readonly #moduleSet = new Set<Module>();
  readonly #chunkSet = new Set<Chunk>();
  readonly #assetValues = Object.create(null) as Record<string, Source>;
  readonly assets: Record<string, Source> = new Proxy(this.#assetValues, {
    set: (target, property, value) => {
      Reflect.set(target, property, value);
      if (typeof property === "string" && !this.#assetMutationActive) {
        this.#assetMutationPending = true;
      }
      return true;
    },
    deleteProperty: (target, property) => {
      const deleted = Reflect.deleteProperty(target, property);
      if (deleted && typeof property === "string" && !this.#assetMutationActive) {
        this.#assetMutationPending = true;
      }
      return deleted;
    }
  });
  #assetMutationActive = false;
  #assetMutationPending = false;
  #assetsMaterialized = false;

  constructor(compilation: NativeCompilation | null | undefined) {
    this.moduleGraph = new ModuleGraphImpl(this.#modulesByHandle);
    this.chunkGraph = new ChunkGraphImpl(
      undefined,
      this.#modulesByHandle,
      this.#chunksByHandle
    );
    this.modules = this.#moduleSet;
    this.chunks = this.#chunkSet;
    this.update(compilation);
  }

  update(compilation: NativeCompilation | null | undefined): void {
    for (const module of compilation?.modules() ?? []) {
      const existing = this.#modulesByHandle.get(module.handle);
      if (existing) {
        existing.updateExports(
          module.providedExports ?? null,
          module.usedExports ?? null,
          module.allExportsUsed ?? false
        );
      } else {
        const moduleImpl = new ModuleImpl(
          module.handle,
          module.resource,
          module.type,
          module.providedExports ?? null,
          module.usedExports ?? null,
          module.allExportsUsed ?? false,
          module.identifier
        );
        moduleImpl.bindModuleGraph(this.moduleGraph);
        this.#modulesByHandle.set(module.handle, moduleImpl);
        this.#moduleSet.add(moduleImpl);
      }
    }
    for (const chunk of compilation?.chunks() ?? []) {
      if (!this.#chunksByHandle.has(chunk.handle)) {
        this.#chunksByHandle.set(
          chunk.handle,
          new ChunkImpl(
          chunk.handle,
          chunk.renderId ?? chunk.render_id ?? null,
          chunk.name ?? undefined
        )
        );
        this.#chunkSet.add(this.#chunksByHandle.get(chunk.handle)!);
      }
    }
    const namedChunkGroups = new Map<string, ChunkGroupImpl>();
    const entrypoints = new Map<string, EntrypointImpl>();
    this.chunkGroups = (compilation?.chunkGroups() ?? []).map((group) => {
      const chunks = group.chunkHandles.flatMap((handle) => {
        const chunk = this.#chunksByHandle.get(handle);
        return chunk ? [chunk] : [];
      });
      const runtimeChunk = group.runtimeChunkHandle == null
        ? null
        : this.#chunksByHandle.get(group.runtimeChunkHandle) ?? null;
      const facade = group.isEntrypoint
        ? new EntrypointImpl(chunks, group.files, runtimeChunk)
        : new ChunkGroupImpl(chunks, group.files);
      if (group.name) namedChunkGroups.set(group.name, facade);
      if (group.name && facade instanceof EntrypointImpl) entrypoints.set(group.name, facade);
      return facade;
    });
    this.namedChunkGroups = namedChunkGroups;
    this.entrypoints = entrypoints;
    if (!this.#assetsMaterialized) {
      const assetSources = compilation?.takeAssetSources() ?? [];
      if (assetSources.length > 0) {
        this.#replaceAssets(assetSources);
        this.#assetsMaterialized = true;
      }
    } else {
      compilation?.clearAssetSources();
    }
    this.moduleGraph.updateNativeCompilation(compilation ?? undefined);
    this.chunkGraph.updateNativeCompilation(compilation ?? undefined);
  }

  releaseNativeCompilation(): void {
    this.moduleGraph.releaseNativeCompilation();
    this.chunkGraph.updateNativeCompilation(undefined);
  }

  beginProcessAssets(assets: NativeAssetSource[]): void {
    const pendingAssets = this.#assetMutationPending
      ? Object.entries(this.assets)
      : [];
    this.#replaceAssets(assets);
    this.#assetMutationActive = true;
    for (const [name, source] of pendingAssets) this.assets[name] = source;
    this.#assetsMaterialized = true;
    this.#assetMutationPending = false;
  }

  endProcessAssets(): void {
    this.#assetMutationActive = false;
  }

  hasPendingAssetMutations(): boolean {
    return this.#assetMutationPending;
  }

  serializeAssets(): NativeAssetSource[] {
    return Object.entries(this.assets).map(([name, source]) => {
      assertSource(source, `assets[${JSON.stringify(name)}]`);
      const buffer = source.buffer();
      return { name, source: Buffer.isBuffer(buffer) ? buffer : Buffer.from(buffer) };
    });
  }

  emitAsset(name: string, source: Source): void {
    assertNonEmptyString(name, "name");
    assertSource(source, "source");
    this.assets[name] = source;
  }

  updateAsset(name: string, source: Source | ((source: Source) => Source)): void {
    assertNonEmptyString(name, "name");
    const current = this.assets[name];
    if (!current) throw new Error(`asset ${JSON.stringify(name)} does not exist`);
    const updated = typeof source === "function" ? source(current) : source;
    assertSource(updated, "source");
    this.assets[name] = updated;
  }

  getAsset(name: string): CompilationAsset | undefined {
    const source = this.assets[name];
    return source ? { name, source } : undefined;
  }

  #replaceAssets(assets: NativeAssetSource[]): void {
    for (const name of Object.keys(this.#assetValues)) delete this.#assetValues[name];
    for (const asset of assets) {
      const source = Buffer.isBuffer(asset.source)
        ? asset.source
        : Buffer.from(asset.source);
      this.#assetValues[asset.name] = new RawSource(source);
    }
  }

}

export function assertSource(value: unknown, name: string): asserts value is Source {
  if (typeof value !== "object" || value === null ||
      typeof (value as { source?: unknown }).source !== "function" ||
      typeof (value as { buffer?: unknown }).buffer !== "function") {
    throw new TypeError(`${name} must be a webpack Source`);
  }
}

export function addToSetMap<TKey, TValue>(
  map: Map<TKey, Set<TValue>>,
  key: TKey,
  value: TValue
): void {
  let values = map.get(key);
  if (!values) {
    values = new Set();
    map.set(key, values);
  }
  values.add(value);
}

export function removeFromSetMap<TKey, TValue>(
  map: Map<TKey, Set<TValue>>,
  key: TKey,
  value: TValue
): void {
  const values = map.get(key);
  if (!values) return;
  values.delete(value);
  if (values.size === 0) map.delete(key);
}

export function groupConnections<TKey>(
  connections: Iterable<ModuleGraphConnection>,
  getKey: (connection: ModuleGraphConnection) => TKey
): ReadonlyMap<TKey, readonly ModuleGraphConnection[]> {
  const groups = new Map<TKey, ModuleGraphConnection[]>();
  for (const connection of connections) {
    const key = getKey(connection);
    const group = groups.get(key);
    if (group) group.push(connection);
    else groups.set(key, [connection]);
  }
  return groups;
}
