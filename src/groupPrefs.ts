// Persisted group preferences shared by the grouped index views (Sessions, PRs, Todo, Schedules):
// custom drag order per grouping kind (`project`, `repo`, `worktree`, defaulting to alphabetical)
// and per-view collapsed group headers. Searching temporarily opens collapsed groups so matching
// rows are never hidden, matching sectionFolds.ts.

import { useCallback, useMemo } from 'react';
import { groupsOf, reorderGroup, type GroupBy, type Grouped, type ListGroup, type ListGroupBy } from './groups';
import { groupOfRow, toggleFoldList, unfoldList } from './sectionFolds';
import { strings, useStored, type Codec } from './stored';
import type { GroupedSessions } from './ui/SessionsView';

export const GROUP_ORDER_KEY = 'codealong.groupOrder';
export const GROUP_COLLAPSED_KEY = 'codealong.groupCollapsed';

export type GroupOrders = Record<GroupBy, string[]>;

const EMPTY_ORDERS: GroupOrders = { project: [], repo: [], worktree: [] };

const listOf = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : []);

export const groupOrdersCodec: Codec<GroupOrders> = {
  parse: (raw) => {
    const v: unknown = JSON.parse(raw);
    if (!v || typeof v !== 'object' || Array.isArray(v)) return undefined;
    const o = v as Record<string, unknown>;
    return { project: listOf(o.project), repo: listOf(o.repo), worktree: listOf(o.worktree) };
  },
  format: (value) => JSON.stringify(value),
};

/** Storage key for a group's collapsed state in a specific view. */
export function groupCollapseKey(view: string, fold: string): string {
  return `${view}:${fold}`;
}

/** Whether a group is collapsed right now: searching temporarily expands every collapsed group. */
export function isGroupCollapsed(view: string, fold: string, collapsed: ReadonlySet<string>, searching: boolean): boolean {
  return !searching && collapsed.has(groupCollapseKey(view, fold));
}

/** The reorderable (non-empty-key) group keys currently displayed in `grouped`. */
export function orderedGroupKeys(grouped: { groups: readonly { key: string }[] } | null | undefined): string[] {
  return grouped ? grouped.groups.filter((g) => g.key !== '').map((g) => g.key) : [];
}

/** Finds the group fold keys that hide `selectedKey` in `grouped`, for revealing a row. */
export function groupFoldsHolding<S, R extends { key: string }>(grouped: Grouped<S, R> | null | undefined, selectedKey: string): string[] {
  if (!grouped) return [];
  const g = groupsOf(grouped).find((x) => x.all.some((r) => r.key === selectedKey));
  return g ? [g.fold] : [];
}

/** Finds the session group fold keys holding `selectedKey` (or any session in `chainKeys`). */
export function sessionGroupFoldsHolding(grouped: GroupedSessions | null | undefined, selectedKey: string, chainKeys: () => string[]): string[] {
  if (!grouped) return [];
  const direct = groupOfRow(selectedKey, grouped);
  if (direct) return [direct.fold];
  return chainKeys()
    .map((k) => groupOfRow(k, grouped)?.fold)
    .filter((f): f is string => !!f);
}

/** Builds a `onPlace` handler for a grouped view, or undefined when ungrouped. */
export function placeGroupFor(prefs: GroupPrefs, by: ListGroupBy, grouped: { groups: readonly { key: string }[] } | null | undefined): ((key: string, index: number) => void) | undefined {
  if (by === 'none') return undefined;
  const shown = orderedGroupKeys(grouped);
  return (key, index) => prefs.placeGroup(by, shown, key, index);
}

export interface GroupPrefs {
  orders: GroupOrders;
  /** Reorders `key` to insertion point `index` among the visible reorderable group keys of `by`. */
  placeGroup: (by: GroupBy, shown: readonly string[], key: string, index: number) => void;
  /** Whether group `g` is collapsed in `view` right now. */
  isCollapsed: (view: string, g: Pick<ListGroup<unknown>, 'fold'>) => boolean;
  /** Click handler to toggle group `g` in `view`, or undefined while searching. */
  toggleFor: (view: string) => ((g: Pick<ListGroup<unknown>, 'fold'>) => void) | undefined;
  /** Ensures group `fold` is open in `view` (when revealing or stepping to a row inside it). */
  uncollapse: (view: string, fold: string) => void;
}

export function useGroupPrefs(searching: boolean): GroupPrefs {
  const [orders, setOrders] = useStored(GROUP_ORDER_KEY, groupOrdersCodec, EMPTY_ORDERS);
  const [collapsedList, setCollapsedList] = useStored(GROUP_COLLAPSED_KEY, strings, [] as string[]);
  const collapsedSet = useMemo(() => new Set(collapsedList), [collapsedList]);

  const placeGroup = useCallback(
    (by: GroupBy, shown: readonly string[], key: string, index: number) => {
      setOrders((prev) => {
        const next = reorderGroup(shown, prev[by], key, index);
        if (next.length === prev[by].length && next.every((k, i) => k === prev[by][i])) return prev;
        return { ...prev, [by]: next };
      });
    },
    [setOrders],
  );

  const isCollapsed = useCallback((view: string, g: Pick<ListGroup<unknown>, 'fold'>) => isGroupCollapsed(view, g.fold, collapsedSet, searching), [collapsedSet, searching]);

  const toggleFor = useCallback(
    (view: string) =>
      searching
        ? undefined
        : (g: Pick<ListGroup<unknown>, 'fold'>) => {
            const k = groupCollapseKey(view, g.fold);
            setCollapsedList((prev) => toggleFoldList(prev, k));
          },
    [searching, setCollapsedList],
  );

  const uncollapse = useCallback(
    (view: string, fold: string) => {
      const k = groupCollapseKey(view, fold);
      setCollapsedList((prev) => unfoldList(prev, k));
    },
    [setCollapsedList],
  );

  return useMemo(() => ({ orders, placeGroup, isCollapsed, toggleFor, uncollapse }), [orders, placeGroup, isCollapsed, toggleFor, uncollapse]);
}
