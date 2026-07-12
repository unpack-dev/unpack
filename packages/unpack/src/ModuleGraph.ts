// Organized to match webpack's lib/ModuleGraph.js responsibility.

import { NativeCompilation, NativeModuleGraphConnection } from "./binding.js";
import { addToSetMap, groupConnections, removeFromSetMap } from "./Compilation.js";
import { Dependency, DependencyImpl } from "./Dependency.js";
import { EMPTY_CONNECTIONS, EMPTY_EXPORTS_INFO, EMPTY_INCOMING_CONNECTION_GROUPS, EMPTY_OPTIMIZATION_BAILOUTS, ExportInfo, ExportsInfo, ExportsInfoImpl } from "./ExportsInfo.js";
import { Module, ModuleImpl } from "./Module.js";
import { ModuleGraphConnection, ModuleGraphConnectionImpl } from "./ModuleGraphConnection.js";

export interface ModuleGraph {
  getResolvedModule(dependency: Dependency): Module | null;
  getConnection(dependency: Dependency): ModuleGraphConnection | undefined;
  getModule(dependency: Dependency): Module | null;
  getOrigin(dependency: Dependency): Module | null;
  getResolvedOrigin(dependency: Dependency): Module | null;
  getParentModule(dependency: Dependency): Module | undefined;
  getParentBlock(dependency: Dependency): undefined;
  getParentBlockIndex(dependency: Dependency): number;
  getIncomingConnections(module: Module): ReadonlySet<ModuleGraphConnection>;
  getOutgoingConnections(module: Module): ReadonlySet<ModuleGraphConnection>;
  getIncomingConnectionsByOriginModule(
    module: Module
  ): ReadonlyMap<Module | null, readonly ModuleGraphConnection[]>;
  getOutgoingConnectionsByModule(
    module: Module
  ): ReadonlyMap<Module, readonly ModuleGraphConnection[]> | undefined;
  getIssuer(module: Module): Module | null | undefined;
  getOptimizationBailout(module: Module): readonly string[];
  getProvidedExports(module: Module): string[] | null;
  isExportProvided(
    module: Module,
    exportName: string | readonly string[]
  ): boolean | null;
  getExportsInfo(module: Module): ExportsInfo;
  getExportInfo(module: Module, exportName: string): ExportInfo;
  getReadOnlyExportInfo(module: Module, exportName: string): ExportInfo;
  getUsedExports(module: Module, runtime?: unknown): ReadonlySet<string> | boolean | null;
  cached<TArgs extends unknown[], TResult>(
    fn: (moduleGraph: ModuleGraph, ...args: TArgs) => TResult,
    ...args: TArgs
  ): TResult;
}

export class ModuleGraphImpl implements ModuleGraph {
  #nativeBinding:
    | { kind: "unbound" }
    | { kind: "active"; compilation: NativeCompilation }
    | { kind: "released" } = { kind: "unbound" };
  readonly #modulesByHandle: ReadonlyMap<number, ModuleImpl>;
  readonly #connectionByHandle = new Map<number, ModuleGraphConnectionImpl>();
  readonly #connectionByDependency = new Map<Dependency, ModuleGraphConnectionImpl>();
  readonly #incoming = new Map<Module, Set<ModuleGraphConnection>>();
  readonly #outgoing = new Map<Module, Set<ModuleGraphConnection>>();
  readonly #incomingByOrigin = new Map<
    Module,
    ReadonlyMap<Module | null, readonly ModuleGraphConnection[]>
  >();
  readonly #outgoingByModule = new Map<
    Module,
    ReadonlyMap<Module, readonly ModuleGraphConnection[]>
  >();
  readonly #issuers = new Map<Module, Module | null>();
  readonly #exports = new Map<Module, ExportsInfoImpl>();
  readonly #loadedIncoming = new Set<Module>();
  readonly #loadedOutgoing = new Set<Module>();

  constructor(modulesByHandle: ReadonlyMap<number, ModuleImpl>) {
    this.#modulesByHandle = modulesByHandle;
    for (const module of modulesByHandle.values()) {
      this.#exports.set(
        module,
        new ExportsInfoImpl(
          module.providedExports,
          module.usedExports,
          module.allExportsUsed
        )
      );
    }
  }

  updateNativeCompilation(nativeCompilation: NativeCompilation | undefined): void {
    if (nativeCompilation) {
      this.#nativeBinding = { kind: "active", compilation: nativeCompilation };
      const materializedHandles = [...this.#connectionByHandle.keys()];
      if (materializedHandles.length > 0) {
        this.#synchronizeConnections(
          nativeCompilation.connectionsByHandle(materializedHandles)
        );
      }
    } else {
      this.#nativeBinding = { kind: "unbound" };
    }
    this.#loadedIncoming.clear();
    this.#loadedOutgoing.clear();
    for (const module of this.#modulesByHandle.values()) {
      this.#exports.set(
        module,
        new ExportsInfoImpl(
          module.providedExports,
          module.usedExports,
          module.allExportsUsed
        )
      );
    }
  }

  releaseNativeCompilation(): void {
    this.#nativeBinding = { kind: "released" };
    this.#loadedIncoming.clear();
    this.#loadedOutgoing.clear();
  }

  #nativeCompilationForRead(): NativeCompilation | undefined {
    if (this.#nativeBinding.kind === "released") {
      throw new Error("module graph lease has been released");
    }
    return this.#nativeBinding.kind === "active"
      ? this.#nativeBinding.compilation
      : undefined;
  }

  #materializeConnection(
    nativeConnection: NativeModuleGraphConnection
  ): ModuleGraphConnectionImpl | undefined {
    const target = this.#modulesByHandle.get(nativeConnection.moduleHandle);
    if (!target) return undefined;
    const existing = this.#connectionByHandle.get(nativeConnection.handle);
    if (existing) {
      if (existing.module !== target) {
        removeFromSetMap(this.#incoming, existing.module, existing);
        addToSetMap(this.#incoming, target, existing);
        existing.updateModule(target);
      }
      return existing;
    }
    const originHandle = nativeConnection.originModuleHandle;
    const origin = originHandle == null
      ? null
      : this.#modulesByHandle.get(originHandle) ?? null;
    const resolvedTarget = this.#modulesByHandle.get(
      nativeConnection.resolvedModuleHandle
    );
    if (!resolvedTarget) return undefined;
    const dependency = new DependencyImpl(
      nativeConnection.dependencyType ?? nativeConnection.dependency_type ?? "unknown",
      nativeConnection.request ?? undefined,
      nativeConnection.weak,
      nativeConnection.parentBlockIndex ?? nativeConnection.parent_block_index ?? -1
    );
    const connection = new ModuleGraphConnectionImpl(
      origin,
      dependency,
      resolvedTarget,
      nativeConnection.weak
    );
    connection.updateModule(target);
    this.#connectionByHandle.set(nativeConnection.handle, connection);
    this.#connectionByDependency.set(dependency, connection);
    addToSetMap(this.#incoming, target, connection);
    if (origin) addToSetMap(this.#outgoing, origin, connection);
    return connection;
  }

  #synchronizeConnections(
    nativeConnections: readonly NativeModuleGraphConnection[]
  ): void {
    for (const nativeConnection of nativeConnections) {
      this.#materializeConnection(nativeConnection);
    }
    this.#incomingByOrigin.clear();
    this.#outgoingByModule.clear();
    this.#issuers.clear();
  }

  getResolvedModule(dependency: Dependency): Module | null {
    return this.getConnection(dependency)?.resolvedModule ?? null;
  }

  getConnection(dependency: Dependency): ModuleGraphConnectionImpl | undefined {
    return this.#connectionByDependency.get(dependency);
  }

  getModule(dependency: Dependency): Module | null {
    return this.getConnection(dependency)?.module ?? null;
  }

  getOrigin(dependency: Dependency): Module | null {
    return this.getConnection(dependency)?.originModule ?? null;
  }

  getResolvedOrigin(dependency: Dependency): Module | null {
    return this.getConnection(dependency)?.resolvedOriginModule ?? null;
  }

  getParentModule(dependency: Dependency): Module | undefined {
    return this.getConnection(dependency)?.originModule ?? undefined;
  }

  getParentBlock(_dependency: Dependency): undefined {
    return undefined;
  }

  getParentBlockIndex(dependency: Dependency): number {
    return dependency instanceof DependencyImpl ? dependency.parentBlockIndex : -1;
  }

  getIncomingConnections(module: Module): ReadonlySet<ModuleGraphConnection> {
    const nativeCompilation = this.#nativeCompilationForRead();
    if (module instanceof ModuleImpl && !this.#loadedIncoming.has(module)) {
      if (nativeCompilation) {
        this.#synchronizeConnections(
          nativeCompilation.incomingConnections(module.nativeHandle)
        );
      }
      this.#loadedIncoming.add(module);
    }
    return this.#incoming.get(module) ?? EMPTY_CONNECTIONS;
  }

  getOutgoingConnections(module: Module): ReadonlySet<ModuleGraphConnection> {
    const nativeCompilation = this.#nativeCompilationForRead();
    if (module instanceof ModuleImpl && !this.#loadedOutgoing.has(module)) {
      if (nativeCompilation) {
        this.#synchronizeConnections(
          nativeCompilation.outgoingConnections(module.nativeHandle)
        );
      }
      this.#loadedOutgoing.add(module);
    }
    return this.#outgoing.get(module) ?? EMPTY_CONNECTIONS;
  }

  getIncomingConnectionsByOriginModule(
    module: Module
  ): ReadonlyMap<Module | null, readonly ModuleGraphConnection[]> {
    let groups = this.#incomingByOrigin.get(module);
    if (!groups) {
      groups = groupConnections(
        this.getIncomingConnections(module),
        (connection) => connection.originModule
      );
      this.#incomingByOrigin.set(module, groups);
    }
    return groups ?? EMPTY_INCOMING_CONNECTION_GROUPS;
  }

  getOutgoingConnectionsByModule(
    module: Module
  ): ReadonlyMap<Module, readonly ModuleGraphConnection[]> | undefined {
    const outgoing = this.getOutgoingConnections(module);
    if (outgoing.size === 0) return undefined;
    let groups = this.#outgoingByModule.get(module);
    if (!groups) {
      groups = groupConnections(outgoing, (connection) => connection.module);
      this.#outgoingByModule.set(module, groups);
    }
    return groups;
  }

  getIssuer(module: Module): Module | null | undefined {
    if (!this.#issuers.has(module)) {
      const incoming = this.getIncomingConnections(module);
      if (incoming.size === 0) return undefined;
      this.#issuers.set(
        module,
        [...incoming].find((connection) => connection.originModule !== null)
          ?.originModule ?? null
      );
    }
    return this.#issuers.get(module);
  }

  getOptimizationBailout(_module: Module): readonly string[] {
    return EMPTY_OPTIMIZATION_BAILOUTS;
  }

  getProvidedExports(module: Module): string[] | null {
    return this.getExportsInfo(module).getProvidedExports();
  }

  isExportProvided(
    module: Module,
    exportName: string | readonly string[]
  ): boolean | null {
    return this.getExportsInfo(module).isExportProvided(exportName);
  }

  getExportsInfo(module: Module): ExportsInfoImpl {
    return this.#exports.get(module) ?? EMPTY_EXPORTS_INFO;
  }

  getExportInfo(module: Module, exportName: string): ExportInfo {
    return this.getExportsInfo(module).getExportInfo(exportName);
  }

  getReadOnlyExportInfo(module: Module, exportName: string): ExportInfo {
    return this.getExportsInfo(module).getReadOnlyExportInfo(exportName);
  }

  getUsedExports(module: Module, runtime?: unknown): ReadonlySet<string> | boolean | null {
    return this.getExportsInfo(module).getUsedExports(runtime);
  }

  cached<TArgs extends unknown[], TResult>(
    fn: (moduleGraph: ModuleGraph, ...args: TArgs) => TResult,
    ...args: TArgs
  ): TResult {
    return fn(this, ...args);
  }
}
