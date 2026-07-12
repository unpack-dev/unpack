// Shared validation and hook-ordering helpers for the JavaScript wrapper.

import { isAbsolute, resolve } from "node:path";

import { Mode } from "./config/normalization.js";

export interface TapOptions {
  name: string;
  stage?: number;
  before?: string | string[];
}

export interface OrderedTap {
  name: string;
  stage: number;
  before: Set<string>;
}

export function insertOrderedTap<TTap extends OrderedTap>(taps: TTap[], tap: TTap): void {
  const before = new Set(tap.before);
  let index = taps.length;
  while (index > 0) {
    const current = taps[index - 1];
    if (before.has(current.name)) {
      before.delete(current.name);
      index -= 1;
      continue;
    }
    if (before.size > 0 || current.stage > tap.stage) {
      index -= 1;
      continue;
    }
    break;
  }
  taps.splice(index, 0, tap);
}

export function normalizeTapOptions(options: string | TapOptions): OrderedTap {
  if (typeof options === "string") {
    return { name: assertNonEmptyString(options, "options"), stage: 0, before: new Set() };
  }
  assertPlainObject(options, "options");
  const name = assertNonEmptyString(options.name, "options.name");
  const stage = options.stage === undefined ? 0 : options.stage;
  if (typeof stage !== "number" || !Number.isFinite(stage)) {
    throw new TypeError("options.stage must be a finite number");
  }
  const before = options.before === undefined
    ? []
    : typeof options.before === "string"
      ? [options.before]
      : options.before;
  if (!Array.isArray(before) || before.some((item) => typeof item !== "string")) {
    throw new TypeError("options.before must be a string or an array of strings");
  }
  return { name, stage, before: new Set(before) };
}

export function assertKnownKeys(
  value: Record<string, unknown>,
  keys: string[],
  name: string
): void {
  const allowed = new Set(keys);
  const unknown = Object.keys(value).filter((key) => !allowed.has(key));
  if (unknown.length > 0) {
    throw new TypeError(`${name} contains unknown option '${unknown[0]}'`);
  }
}

export function assertPlainObject(value: unknown, name: string): asserts value is Record<string, unknown> {
  if (
    typeof value !== "object" ||
    value === null ||
    Array.isArray(value)
  ) {
    throw new TypeError(`${name} must be an object`);
  }
}

export function assertString(value: unknown, name: string): string {
  if (typeof value !== "string") {
    throw new TypeError(`${name} must be a string`);
  }
  return value;
}

export function normalizePath(
  value: unknown,
  name: string,
  context: string,
  requireAbsolute = false
): string {
  const path = assertString(value, name);
  if (requireAbsolute && !isAbsolute(path)) {
    throw new TypeError(`${name} must be an absolute path`);
  }
  return isAbsolute(path) ? path : resolve(context, path);
}

export function assertCacheType(value: unknown): "memory" | "filesystem" {
  if (value !== "memory" && value !== "filesystem") {
    throw new TypeError("options.cache.type must be 'memory' or 'filesystem'");
  }
  return value;
}

export function assertMode(value: unknown): Mode {
  if (value !== "development" && value !== "production" && value !== "none") {
    throw new TypeError("options.mode must be 'development', 'production', or 'none'");
  }
  return value;
}

export function assertBoolean(value: unknown, name: string): boolean {
  if (typeof value !== "boolean") {
    throw new TypeError(`${name} must be a boolean`);
  }
  return value;
}

export function assertNonNegativeInteger(value: unknown, name: string): number {
  if (
    typeof value !== "number" ||
    !Number.isInteger(value) ||
    value < 0
  ) {
    throw new TypeError(`${name} must be a non-negative integer`);
  }
  return value;
}

export function assertNonNegativeNumber(value: unknown, name: string): number {
  if (typeof value !== "number" || Number.isNaN(value) || value < 0) {
    throw new TypeError(`${name} must be a non-negative number`);
  }
  return value;
}

export function assertCacheCompression(value: unknown): "gzip" | "brotli" {
  if (value === "gzip" || value === "brotli") return value;
  throw new TypeError("options.cache.compression must be false, 'gzip', or 'brotli'");
}

export function normalizeGenerationLimit(
  value: unknown,
  name: string,
  allowZero: boolean
): number | undefined {
  const valid =
    typeof value === "number" &&
    !Number.isNaN(value) &&
    value !== Number.NEGATIVE_INFINITY &&
    (allowZero ? value >= 0 : value >= 1);
  if (!valid) {
    throw new TypeError(
      `${name} must be ${allowZero ? "non-negative" : "at least 1"}`
    );
  }
  if (value === Number.POSITIVE_INFINITY) {
    return undefined;
  }

  return Math.ceil(value);
}

export function assertPositiveInteger(value: unknown, name: string): number {
  if (
    typeof value !== "number" ||
    !Number.isInteger(value) ||
    value <= 0
  ) {
    throw new TypeError(`${name} must be a positive integer`);
  }
  return value;
}

export function assertNonEmptyString(value: unknown, name: string): string {
  const string = assertString(value, name);
  if (string.length === 0) {
    throw new TypeError(`${name} must not be empty`);
  }
  return string;
}

export function assertFunction(value: unknown, name: string): asserts value is Function {
  if (typeof value !== "function") {
    throw new TypeError(`${name} must be a function`);
  }
}

export function defer(callback: () => void): void {
  queueMicrotask(callback);
}

export function namedError(name: string, message: string): Error {
  const error = new Error(message);
  error.name = name;
  return error;
}

export function toError(error: unknown, name: string): Error {
  if (error instanceof Error) {
    error.name = error.name === "Error" ? name : error.name;
    return error;
  }
  return namedError(name, String(error));
}
