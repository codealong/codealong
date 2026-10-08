// Picking several rows of a list together, as a Mac list does: ⌘-click adds a row or leaves it
// out, ⇧-click (and ⇧↑ ⇧↓) picks the rows from where the run started to it, Esc goes back to one.
// Todos picked together start in one session (useTodoView.ts); sessions picked together are
// marked seen, snoozed, closed, done or archived at once (menus.ts `sessionsActions`).

import { useMemo, useState } from 'react';

/** How a press selects a row: alone, ⌘ to add it or leave it out, ⇧ for the rows up to it. */
export type SelectHow = 'one' | 'toggle' | 'range';

/** How a press with these keys held selects a row. */
export function selectHow(e: { metaKey: boolean; shiftKey: boolean }): SelectHow {
  return e.metaKey ? 'toggle' : e.shiftKey ? 'range' : 'one';
}

/** Rows picked together. It holds while `focus` is the selected row: selecting another row any
 *  other way ends it. */
export interface RowPick {
  /** The row the keyboard is on; the panel's when only one is picked. */
  focus: string;
  /** Where a ⇧-click or ⇧↑ ⇧↓ run starts. */
  anchor: string;
  keys: string[];
}

/** The rows from `from` to `to`, both included, in the list's order. */
function rangeOf(order: string[], from: string, to: string): string[] {
  const a = order.indexOf(from);
  const b = order.indexOf(to);
  if (a < 0 || b < 0) return [to];
  return order.slice(Math.min(a, b), Math.max(a, b) + 1);
}

/** ⇧-click, or ⇧↑ ⇧↓: the rows from the anchor to `to`. Without a pick, the anchor is the row
 *  selected before. */
export function pickTo(order: string[], pick: RowPick | null, selected: string | null, to: string): RowPick {
  const anchor = pick?.anchor ?? selected ?? to;
  return { focus: to, anchor, keys: rangeOf(order, anchor, to) };
}

/** ⌘-click: adds a row to the pick, or leaves it out. Leaving out the row the keyboard is on
 *  moves it to the nearest row still picked, in the list's order. */
export function pickToggle(order: string[], pick: RowPick | null, selected: string | null, key: string): RowPick {
  const keys = pick?.keys ?? (selected ? [selected] : []);
  if (!keys.includes(key)) return { focus: key, anchor: key, keys: [...keys, key] };
  const left = keys.filter((k) => k !== key);
  if (left.length === 0) return { focus: key, anchor: key, keys: [key] };
  if (pick && pick.focus !== key && left.includes(pick.focus)) return { ...pick, keys: left };
  const at = order.indexOf(key);
  const inOrder = [...left].sort((x, y) => order.indexOf(x) - order.indexOf(y));
  const focus = inOrder.find((k) => order.indexOf(k) > at) ?? inOrder[inOrder.length - 1];
  return { focus, anchor: focus, keys: left };
}

/** The row to select once `gone` leave the list: the first after the last of them that stays,
 *  else the nearest before the first. Null when none stays. */
export function nextAfter(order: string[], gone: ReadonlySet<string>): string | null {
  const at = order.flatMap((k, i) => (gone.has(k) ? [i] : []));
  const after = order.slice((at[at.length - 1] ?? -1) + 1).find((k) => !gone.has(k));
  return after ?? order.slice(0, at[0] ?? 0).reverse().find((k) => !gone.has(k)) ?? null;
}

/** Rows picked together in a list, which lists `rows` in its order (null while it isn't shown),
 *  with `selected` the key of the row selected: `picked` is them in the list's order, none when
 *  fewer than two are, `select` selects a row the way a press did, `all` picks them all, and
 *  `leave` lets go of rows done with. */
export function usePicks<R extends { key: string }>(rows: R[] | null, selected: string | null, setSelectedKey: (key: string) => void) {
  const [pick, setPick] = useState<RowPick | null>(null);
  // They hold while the keyboard stays on the row they were picked from.
  const picked = useMemo(() => {
    if (!rows || !pick || pick.focus !== selected) return [];
    const list = rows.filter((r) => pick.keys.includes(r.key));
    return list.length > 1 ? list : [];
  }, [rows, pick, selected]);
  const keys = useMemo(() => new Set(picked.map((r) => r.key)), [picked]);
  const select = (r: R, how: SelectHow) => {
    if (how === 'one') {
      setPick(null);
      setSelectedKey(r.key);
      return;
    }
    const order = (rows ?? []).map((x) => x.key);
    const live = picked.length > 0 ? pick : null;
    const next = how === 'toggle' ? pickToggle(order, live, selected, r.key) : pickTo(order, live, selected, r.key);
    setPick(next);
    setSelectedKey(next.focus);
  };
  /** Takes rows that left the list out of the pick: the keyboard stays on one still picked, or
   *  goes to the row after them. */
  const leave = (gone: ReadonlySet<string>) => {
    const left = picked.map((r) => r.key).filter((k) => !gone.has(k));
    if (left.length === 0) {
      setPick(null);
      const next = nextAfter((rows ?? []).map((x) => x.key), gone);
      if (next) setSelectedKey(next);
      return;
    }
    const focus = pick && left.includes(pick.focus) ? pick.focus : left[0];
    setPick({ focus, anchor: focus, keys: left });
    setSelectedKey(focus);
  };
  /** Picks every row, the keyboard staying on the one it's on. */
  const all = () => {
    if (!rows || rows.length < 2) return;
    const focus = selected && rows.some((r) => r.key === selected) ? selected : rows[0].key;
    setPick({ focus, anchor: rows[0].key, keys: rows.map((r) => r.key) });
    setSelectedKey(focus);
  };
  return { picked, keys, select, leave, all, clear: () => setPick(null) };
}
