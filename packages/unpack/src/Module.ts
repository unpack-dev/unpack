// Organized to match webpack's lib/Module.js responsibility.

import { dirname } from "node:path";

import { Dependency } from "./Dependency.js";
import { ModuleGraphImpl } from "./ModuleGraph.js";

export interface Module {
  readonly context: string | null;
  readonly resource: string;
  readonly type: string;
  readonly dependencies: readonly Dependency[];
  identifier(): string;
  readableIdentifier(): string;
  nameForCondition(): string;
}

export class ModuleImpl implements Module {
  readonly #identifier: string;
  #moduleGraph: ModuleGraphImpl | undefined;
  #dependencies: readonly Dependency[] | undefined;
  providedExports: readonly string[] | null;
  usedExports: readonly string[] | null;
  allExportsUsed: boolean;
  readonly context: string | null;

  constructor(
    readonly nativeHandle: number,
    readonly resource: string,
    readonly type: string,
    providedExports: readonly string[] | null,
    usedExports: readonly string[] | null,
    allExportsUsed: boolean,
    identifier: string
  ) {
    this.#identifier = identifier;
    this.providedExports = providedExports;
    this.usedExports = usedExports;
    this.allExportsUsed = allExportsUsed;
    this.context = dirname(resource.split(/[?#]/, 1)[0]);
  }

  identifier(): string {
    return this.#identifier;
  }

  readableIdentifier(): string {
    return this.#identifier;
  }

  nameForCondition(): string {
    return this.resource;
  }

  get dependencies(): readonly Dependency[] {
    if (!this.#dependencies) {
      this.#dependencies = this.#moduleGraph
        ? [...this.#moduleGraph.getOutgoingConnections(this)].map(
            (connection) => connection.dependency
          )
        : [];
    }
    return this.#dependencies;
  }

  bindModuleGraph(moduleGraph: ModuleGraphImpl): void {
    this.#moduleGraph = moduleGraph;
  }

  updateExports(
    providedExports: readonly string[] | null,
    usedExports: readonly string[] | null,
    allExportsUsed: boolean
  ): void {
    this.providedExports = providedExports;
    this.usedExports = usedExports;
    this.allExportsUsed = allExportsUsed;
  }
}
