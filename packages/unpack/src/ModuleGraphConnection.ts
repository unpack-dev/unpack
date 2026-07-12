// Organized to match webpack's lib/ModuleGraphConnection.js responsibility.

import { Dependency } from "./Dependency.js";
import { Module } from "./Module.js";

export interface ModuleGraphConnection {
  readonly originModule: Module | null;
  readonly resolvedOriginModule: Module | null;
  readonly dependency: Dependency;
  readonly module: Module;
  readonly resolvedModule: Module;
  readonly weak: boolean;
  readonly conditional: false;
  readonly active: boolean;
  readonly explanations: ReadonlySet<string>;
  getActiveState(runtime?: unknown): boolean;
  isActive(runtime?: unknown): boolean;
  isTargetActive(runtime?: unknown): boolean;
}

export class ModuleGraphConnectionImpl implements ModuleGraphConnection {
  readonly resolvedOriginModule: Module | null;
  readonly resolvedModule: Module;
  readonly conditional = false as const;
  readonly active = true;
  readonly explanations: ReadonlySet<string> = new Set();
  #module: Module;

  constructor(
    readonly originModule: Module | null,
    readonly dependency: Dependency,
    module: Module,
    readonly weak: boolean
  ) {
    this.resolvedOriginModule = originModule;
    this.resolvedModule = module;
    this.#module = module;
  }

  get module(): Module {
    return this.#module;
  }

  updateModule(module: Module): void {
    this.#module = module;
  }

  getActiveState(_runtime?: unknown): boolean {
    return true;
  }

  isActive(_runtime?: unknown): boolean {
    return true;
  }

  isTargetActive(_runtime?: unknown): boolean {
    return true;
  }
}
