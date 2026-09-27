// A list row that renders again only when what it shows changed. The board's rows are rebuilt
// on every push (fresh wrapper objects around the same lanes and sessions, fresh callbacks), so
// plain `memo` would never skip one: props are compared a few levels down, and callbacks always
// count as equal while calls go to the newest one.

import { createElement, memo, useRef, type ComponentType, type FunctionComponent } from 'react';

/** True when `a` and `b` hold the same things, looking `depth` levels into plain objects and
 *  arrays. Functions are always equal: `memoRow` calls the latest one. */
export function looseEqual(a: unknown, b: unknown, depth: number): boolean {
  if (a === b) return true;
  if (typeof a === 'function' && typeof b === 'function') return true;
  if (depth <= 0 || typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false;
  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) return false;
    return a.every((x, i) => looseEqual(x, b[i], depth - 1));
  }
  if (Array.isArray(b) || Object.getPrototypeOf(a) !== Object.prototype || Object.getPrototypeOf(b) !== Object.prototype) return false;
  const ka = Object.keys(a);
  if (ka.length !== Object.keys(b).length) return false;
  return ka.every((k) => k in b && looseEqual((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k], depth - 1));
}

/** `Component`, skipping renders whose props are `looseEqual` to the last ones. Functions it
 *  calls while rendering (`reads`) are compared as values instead: the row must render again
 *  when one changes, so pass ones that stay the same while their answers do. */
export function memoRow<P extends object>(Component: ComponentType<P>, reads: (keyof P & string)[] = [], depth = 4): FunctionComponent<P> {
  const Inner = memo(Component, (a, b) => {
    for (const k of reads) if ((a as Record<string, unknown>)[k] !== (b as Record<string, unknown>)[k]) return false;
    return looseEqual(a, b, depth);
  });
  function Row(props: P) {
    // Each callback prop is handed over as a stable stand-in that calls the newest one, so a
    // skipped render never leaves the row calling an old closure.
    const latest = useRef(props);
    latest.current = props;
    const stand = useRef(new Map<string, (...args: unknown[]) => unknown>());
    const passed = { ...props } as Record<string, unknown>;
    for (const k of Object.keys(passed)) {
      if (typeof passed[k] !== 'function' || (reads as string[]).includes(k)) continue;
      let f = stand.current.get(k);
      if (!f) {
        f = (...args: unknown[]) => (latest.current as Record<string, (...a: unknown[]) => unknown>)[k]?.(...args);
        stand.current.set(k, f);
      }
      passed[k] = f;
    }
    return createElement(Inner as unknown as ComponentType<Record<string, unknown>>, passed);
  }
  Row.displayName = `memoRow(${Component.displayName ?? Component.name})`;
  return Row;
}
