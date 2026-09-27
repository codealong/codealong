// Every board push arrives as a fresh object, though most carry one or two changed worktrees.
// Handing React the previous objects wherever nothing changed lets memos and rows skip them.

type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

/** `next`, with every part equal to the same part of `prev` replaced by `prev`'s own object.
 *  Returns `prev` itself when nothing changed. Items in arrays are matched by their `key` or
 *  `id` when they have one, so a worktree that moves in the list is still reused. */
export function reuse<T>(prev: T, next: T): T {
  return same(prev as Json, next as Json) as T;
}

function same(prev: Json, next: Json): Json {
  if (prev === next || typeof prev !== 'object' || typeof next !== 'object' || prev === null || next === null) return next;
  if (Array.isArray(prev) !== Array.isArray(next)) return next;
  return Array.isArray(next) ? sameArray(prev as Json[], next) : sameObject(prev as Record<string, Json>, next as Record<string, Json>);
}

function identity(item: Json): string | number | undefined {
  if (typeof item !== 'object' || item === null || Array.isArray(item)) return undefined;
  const k = item.key ?? item.id;
  return typeof k === 'string' || typeof k === 'number' ? k : undefined;
}

function sameArray(prev: Json[], next: Json[]): Json[] {
  let byId: Map<string | number, Json> | null = null;
  let changed = prev.length !== next.length;
  const out = next.map((item, i) => {
    const id = identity(item);
    let match: Json | undefined = prev[i];
    if (id !== undefined && identity(match ?? null) !== id) {
      if (!byId) {
        byId = new Map();
        for (const p of prev) {
          const pid = identity(p);
          if (pid !== undefined) byId.set(pid, p);
        }
      }
      match = byId.get(id);
    }
    const kept = match === undefined ? item : same(match, item);
    if (kept !== prev[i]) changed = true;
    return kept;
  });
  return changed ? out : prev;
}

function sameObject(prev: Record<string, Json>, next: Record<string, Json>): Record<string, Json> {
  let changed = false;
  let count = 0;
  const out: Record<string, Json> = {};
  for (const k in next) {
    count++;
    const kept = k in prev ? same(prev[k], next[k]) : next[k];
    if (kept !== prev[k] || !(k in prev)) changed = true;
    out[k] = kept;
  }
  if (!changed) {
    for (const _ in prev) count--;
    changed = count !== 0;
  }
  return changed ? out : prev;
}
