// Organized to match webpack's lib/ExportsInfo.js responsibility.

import { Module } from "./Module.js";
import { ModuleGraphConnection } from "./ModuleGraphConnection.js";

export interface ExportInfo {
  readonly name: string;
  readonly provided: boolean | null;
  getUsedName(): string | false;
}

export interface ExportsInfo {
  getProvidedExports(): string[] | null;
  isExportProvided(exportName: string | readonly string[]): boolean | null;
  getExportInfo(exportName: string): ExportInfo;
  getReadOnlyExportInfo(exportName: string): ExportInfo;
  getUsedExports(runtime?: unknown): ReadonlySet<string> | boolean | null;
}

export class ExportInfoImpl implements ExportInfo {
  constructor(
    readonly name: string,
    readonly provided: boolean | null,
    readonly used: boolean | null
  ) {}

  getUsedName(): string | false {
    return this.used === false ? false : this.name;
  }
}

export class ExportsInfoImpl implements ExportsInfo {
  readonly #provided: Set<string>;
  readonly #providedKnown: boolean;
  readonly #used: Set<string> | null;
  readonly #exports = new Map<string, ExportInfo>();

  constructor(
    providedExports: readonly string[] | null,
    usedExports: readonly string[] | null,
    readonly allExportsUsed = false
  ) {
    this.#providedKnown = providedExports !== null;
    this.#provided = new Set(providedExports ?? []);
    this.#used = usedExports === null ? null : new Set(usedExports);
  }

  getProvidedExports(): string[] | null {
    return this.#providedKnown ? [...this.#provided] : null;
  }

  isExportProvided(exportName: string | readonly string[]): boolean | null {
    const name = typeof exportName === "string" ? exportName : exportName[0];
    return this.#providedKnown && name !== undefined ? this.#provided.has(name) : null;
  }

  getExportInfo(exportName: string): ExportInfo {
    let info = this.#exports.get(exportName);
    if (!info) {
      info = new ExportInfoImpl(
        exportName,
        this.#providedKnown ? this.#provided.has(exportName) : null,
        this.#used === null
          ? null
          : this.allExportsUsed || this.#used.has(exportName)
      );
      this.#exports.set(exportName, info);
    }
    return info;
  }

  getReadOnlyExportInfo(exportName: string): ExportInfo {
    return this.getExportInfo(exportName);
  }

  getUsedExports(_runtime?: unknown): ReadonlySet<string> | boolean | null {
    if (this.#used === null) return null;
    if (this.allExportsUsed) return true;
    return this.#used.size === 0 ? false : this.#used;
  }
}

export const EMPTY_CONNECTIONS: ReadonlySet<ModuleGraphConnection> = new Set();

export const EMPTY_INCOMING_CONNECTION_GROUPS: ReadonlyMap<
  Module | null,
  readonly ModuleGraphConnection[]
> = new Map();

export const EMPTY_OPTIMIZATION_BAILOUTS: readonly string[] = [];

export const EMPTY_EXPORTS_INFO = new ExportsInfoImpl([], null);
