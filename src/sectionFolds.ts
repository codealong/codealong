// Collapsible sections in the Sessions view (ui/SessionsView.tsx): Orchestrators, Listening,
// Pinned, Needs you, Your turn, Working, Snoozed, Idle, and Archived. Day dividers in Started sort
// (`NON_FOLDABLE_SECTIONS`) are plain headers and don't fold. Ambient sections
// (`PERSISTED_SECTION_FOLDS`) remember their state across launches via stored.ts; urgent sections
// (Needs you, Your turn, Listening) start open each launch; Archived starts folded each launch
// (`showArchived`). While a search query is active, folded active sections open temporarily and
// can't be toggled so matching rows are never hidden; Archived stays controlled by `showArchived`.

import { useCallback, useMemo, useState } from 'react';
import { DAYS } from './days';
import { groupsOf } from './groups';
import { strings, useStored } from './stored';
import type { ListRow } from './boardLists';
import { groupHeadRows, headItem, type GroupedSessions, type SessionGroup, type SessionItem, type SessionSection } from './ui/SessionsView';

export const SESSION_FOLDS_KEY = 'codealong.sessionFolds';

/** The fixed ambient sections whose collapsed state is remembered across launches. Urgent bays
 *  (`needs`, `unread`, `listening`) start open each launch. */
export const PERSISTED_SECTION_FOLDS: ReadonlySet<string> = new Set(['orchestrators', 'pinned', 'working', 'snoozed', 'idle']);

/** Chronological day headings (Started sort) act as dividers rather than collapsible groups. */
const NON_FOLDABLE_SECTIONS: ReadonlySet<string> = new Set(DAYS.map((d) => d.id));

/** The uncollapsed sections of the Sessions view, for computing visible rows and finding which
 *  section holds a row. */
export interface SessionSectionParts {
  orchestrators: readonly SessionItem[];
  listening: readonly SessionItem[];
  pinned: readonly SessionItem[];
  topHelpers: ReadonlyMap<string, readonly SessionItem[]>;
  sections: readonly SessionSection[];
  groups: GroupedSessions | null;
  archived: readonly SessionItem[];
}

/** Whether section `id` is currently folded. Searching temporarily opens active sections so
 *  matches aren't hidden; Archived stays controlled by `showArchived`; day dividers never fold. */
export function isSectionFolded(id: string, folded: ReadonlySet<string>, showArchived: boolean, searching: boolean): boolean {
  if (id === 'archived') return !showArchived;
  if (NON_FOLDABLE_SECTIONS.has(id)) return false;
  return !searching && folded.has(id);
}

/** Toggles `id` in a stored list of folded section IDs. */
export function toggleFoldList(list: readonly string[], id: string): string[] {
  return list.includes(id) ? list.filter((x) => x !== id) : [...list, id];
}

/** Removes `id` from a stored list of folded section IDs, keeping the same array when absent. */
export function unfoldList(list: string[], id: string): string[] {
  return list.includes(id) ? list.filter((x) => x !== id) : list;
}

const withTopHelpers = (rows: readonly SessionItem[], topHelpers: ReadonlyMap<string, readonly SessionItem[]>): SessionItem[] =>
  rows.flatMap((r) => [r, ...(topHelpers.get(r.session.key) ?? [])]);

/** The Sessions list's visible rows top to bottom, skipping any folded section or collapsed group's
 *  children (a collapsed worktree group still keeps its worktree heading selectable). */
export function visibleSessionRows(parts: SessionSectionParts, folded: (id: string) => boolean, groupCollapsed?: (g: SessionGroup) => boolean): ListRow[] {
  const { orchestrators, listening, pinned, topHelpers, sections, groups, archived } = parts;
  const body: ListRow[] = groups
    ? groups.groups.flatMap((g) => (groupCollapsed?.(g) ? (g.lane ? [headItem(g.lane)] : []) : groupHeadRows(g)))
    : sections.flatMap((s) => (folded(s.id) ? [] : s.rows));
  return [
    ...(folded('orchestrators') ? [] : orchestrators),
    ...(folded('listening') ? [] : withTopHelpers(listening, topHelpers)),
    ...(folded('pinned') ? [] : withTopHelpers(pinned, topHelpers)),
    ...body,
    ...(folded('archived') ? [] : archived),
  ];
}

/** Every active (non-archived) session in `parts`, folded or not: keeps a selected session in the
 *  panel when the user folds its section or group (mainSelection.ts `selectedRow`). */
export function activeSessionRows(parts: SessionSectionParts): SessionItem[] {
  const { orchestrators, listening, pinned, topHelpers, sections, groups } = parts;
  const body = groups ? groupsOf(groups).flatMap((g) => g.all) : sections.flatMap((s) => s.rows);
  return [...orchestrators, ...withTopHelpers(listening, topHelpers), ...withTopHelpers(pinned, topHelpers), ...body];
}

/** Whether the Sessions view has any section or group to show (even if every one is folded), so the
 *  empty-list message only shows when there is genuinely nothing listed. */
export function hasSessionSections(parts: SessionSectionParts, showArchived: boolean): boolean {
  const { orchestrators, listening, pinned, sections, groups, archived } = parts;
  if (orchestrators.length > 0 || listening.length > 0 || pinned.length > 0) return true;
  if (groups ? groups.groups.length > 0 || groups.apart.length > 0 : sections.length > 0) return true;
  return showArchived && archived.length > 0;
}

/** Which section holds row `key`, or null when it isn't in a foldable section (e.g. a row inside
 *  the unified grouped list, whose own group handles collapsing). */
export function sectionOfRow(key: string, parts: SessionSectionParts): string | null {
  const { orchestrators, listening, pinned, topHelpers, sections, groups, archived } = parts;
  if (orchestrators.some((r) => r.key === key)) return 'orchestrators';
  if (withTopHelpers(listening, topHelpers).some((r) => r.key === key)) return 'listening';
  if (withTopHelpers(pinned, topHelpers).some((r) => r.key === key)) return 'pinned';
  if (!groups) {
    for (const s of sections) {
      if (s.rows.some((r) => r.key === key)) return s.id;
    }
  }
  if (archived.some((r) => r.key === key)) return 'archived';
  return null;
}

/** Which group in a grouped Sessions list holds row `key` (a session's `lane::key` or a worktree
 *  heading's `lane`), or null when absent. */
export function groupOfRow(key: string, groups: GroupedSessions | null): SessionGroup | null {
  if (!groups) return null;
  return groupsOf(groups).find((g) => g.lane?.id === key || g.all.some((r) => r.key === key)) ?? null;
}

export interface SectionFolds {
  showArchived: boolean;
  setShowArchived: (show: boolean) => void;
  isFolded: (id: string) => boolean;
  toggle: (id: string) => void;
  /** Click handler for section `id`'s header, or undefined when the section can't toggle right
   *  now (day dividers, and active sections while searching; Archived still toggles). */
  toggleFor: (id: string) => (() => void) | undefined;
  unfold: (id: string) => void;
}

/** Fold state for the Sessions view's sections: persisted for ambient sections, ephemeral for
 *  urgent sections, non-foldable for day dividers, and default-folded for Archived. */
export function useSectionFolds(searching: boolean): SectionFolds {
  const [persisted, setPersisted] = useStored(SESSION_FOLDS_KEY, strings, [] as string[]);
  const [ephemeral, setEphemeral] = useState<ReadonlySet<string>>(() => new Set());
  const [showArchived, setShowArchived] = useState(false);

  const foldedSet = useMemo(() => {
    const out = new Set<string>(ephemeral);
    for (const id of persisted) if (PERSISTED_SECTION_FOLDS.has(id)) out.add(id);
    return out;
  }, [persisted, ephemeral]);

  const isFolded = useCallback((id: string) => isSectionFolded(id, foldedSet, showArchived, searching), [foldedSet, showArchived, searching]);

  const toggle = useCallback(
    (id: string) => {
      if (id === 'archived') {
        setShowArchived((s) => !s);
      } else if (searching || NON_FOLDABLE_SECTIONS.has(id)) {
        return;
      } else if (PERSISTED_SECTION_FOLDS.has(id)) {
        setPersisted((prev) => toggleFoldList(prev, id));
      } else {
        setEphemeral((prev) => {
          const next = new Set(prev);
          if (!next.delete(id)) next.add(id);
          return next;
        });
      }
    },
    [searching, setPersisted],
  );

  const unfold = useCallback(
    (id: string) => {
      if (id === 'archived') {
        setShowArchived(true);
      } else if (PERSISTED_SECTION_FOLDS.has(id)) {
        setPersisted((prev) => unfoldList(prev, id));
      } else {
        setEphemeral((prev) => {
          if (!prev.has(id)) return prev;
          const next = new Set(prev);
          next.delete(id);
          return next;
        });
      }
    },
    [setPersisted],
  );

  const toggleFor = useCallback((id: string) => (!NON_FOLDABLE_SECTIONS.has(id) && (!searching || id === 'archived') ? () => toggle(id) : undefined), [searching, toggle]);

  return { showArchived, setShowArchived, isFolded, toggle, toggleFor, unfold };
}
