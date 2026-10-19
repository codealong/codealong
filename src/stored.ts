// What a window remembers of how you left it (the view, a filter, a panel's width, the project
// picked), each value in localStorage under a key of its own. Storage can be off (a private
// window) or full: a choice then lasts as long as the window, and nothing fails. A key and the
// way its value is written never change, or what people chose is lost with the next update.

import { useCallback, useRef, useState } from 'react';

/** How a value is written to storage and read back. `parse` answers undefined for what it can't
 *  read, and the fallback is used then; `format` answers null to take the key out. */
export interface Codec<T> {
  parse: (raw: string) => T | undefined;
  format: (value: T) => string | null;
}

/** On or off, written `1` or `0`. */
export const flag: Codec<boolean> = { parse: (raw) => raw === '1', format: (on) => (on ? '1' : '0') };

/** Text, as it is. */
export const asText: Codec<string> = { parse: (raw) => raw, format: (value) => value };

/** One of `ids`, written as it is; anything else reads as missing. */
export function oneOf<T extends string>(ids: readonly T[]): Codec<T> {
  return { parse: (raw) => (ids.includes(raw as T) ? (raw as T) : undefined), format: (value) => value };
}

/** A list of strings, as JSON. What isn't a string is left out of what's read. */
export const strings: Codec<string[]> = {
  parse: (raw) => {
    const v: unknown = JSON.parse(raw);
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : undefined;
  },
  format: (value) => JSON.stringify(value),
};

/** Text by name, as a JSON object. What isn't text is left out of what's read. */
export const textByName: Codec<Record<string, string>> = {
  parse: (raw) => {
    const v: unknown = JSON.parse(raw);
    if (!v || typeof v !== 'object' || Array.isArray(v)) return undefined;
    return Object.fromEntries(Object.entries(v).filter((e): e is [string, string] => typeof e[1] === 'string'));
  },
  format: (value) => JSON.stringify(value),
};

/** The value kept under `key`, or `fallback` when there's none, it doesn't read, or storage is off. */
export function readStored<T, F = T>(key: string, codec: Codec<T>, fallback: F): T | F {
  try {
    const raw = window.localStorage.getItem(key);
    if (raw === null) return fallback;
    const value = codec.parse(raw);
    return value === undefined ? fallback : value;
  } catch {
    return fallback;
  }
}

/** Keeps `value` under `key`, or takes the key out when the codec says so. */
export function writeStored<T>(key: string, codec: Codec<T>, value: T): void {
  try {
    const raw = codec.format(value);
    if (raw === null) window.localStorage.removeItem(key);
    else window.localStorage.setItem(key, raw);
  } catch {
    // Storage can be off or full: the value lasts as long as the window.
  }
}

/**
 * State that's read from `key` as the window opens and written back on every change. A null key
 * keeps it in this window alone (a project's own window doesn't change the main window's
 * project). The setter takes a value or a function of the last one, which composes when called
 * twice before a render; it stays the same function while `key` and `codec` do, so the codec
 * should be a constant.
 */
export function useStored<T, F = T>(key: string | null, codec: Codec<T>, fallback: F): [T | F, (next: T | ((prev: T | F) => T)) => void] {
  const [value, setValue] = useState<T | F>(() => (key === null ? fallback : readStored(key, codec, fallback)));
  const last = useRef(value);
  const set = useCallback(
    (next: T | ((prev: T | F) => T)) => {
      const v = typeof next === 'function' ? (next as (prev: T | F) => T)(last.current) : next;
      last.current = v;
      setValue(v);
      if (key !== null) writeStored(key, codec, v);
    },
    [key, codec],
  );
  return [value, set];
}
