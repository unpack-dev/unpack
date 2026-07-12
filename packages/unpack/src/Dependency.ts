// Organized to match webpack's lib/Dependency.js responsibility.

export interface Dependency {
  readonly type: string;
  readonly request?: string;
  readonly weak: boolean;
  getResourceIdentifier(): string | null;
}

export class DependencyImpl implements Dependency {
  constructor(
    readonly type: string,
    readonly request: string | undefined,
    readonly weak: boolean,
    readonly parentBlockIndex: number
  ) {}

  getResourceIdentifier(): string | null {
    return this.request === undefined ? null : `context|module${this.request}`;
  }
}
