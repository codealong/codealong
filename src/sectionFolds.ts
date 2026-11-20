// Collapsible sections in the Sessions view (ui/SessionsView.tsx): Orchestrators, Listening,
// Pinned, Pinned groups, Needs you, Your turn, Working, Snoozed, each day section, and Archived.
// Ambient sections (`PERSISTED_SECTION_FOLDS`) remember their state across launches via stored.ts;
// urgent sections (Needs you, Your turn, Listening) and rotating day buckets start open each
// launch; Archived starts folded each launch (`showArchived`). While a search query is active,
// folded active sections open temporarily so matching rows are never hidden; Archived stays
// folded until opened or revealed.

import { useCallback, useMemo, useState } from 'react';
import { hearingShows } from './listening';
import { listedUnder } from './lanes';
import { strings, useStored } from './stored';
import type { ListRow } from './boardLists';
import { groupKeyOf, sessionSections, type GroupedSessions, type SessionGroupBy, type SessionItem, type SessionSection, type SessionSort, type StartedPlace } from './ui/SessionsView';
import { headItem } from './ui/SessionsView';

export const SESSION_FOLDS_KEY = 'codealong.sessionFolds';

/** The fixed ambient sections whose collapsed state is remembered across launches. Urgent bays
 *  (`needs`, `unread`, `listening`) and rotating day headings start open each launch. */
export const PERSISTED_SECTION_FOLDS: ReadonlySet<string> = new Set(['orchestrators', 'pinned', 'pinnedGroups', 'working', 'snoozed']);

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
 *  matches aren't hidden; Archived stays controlled by `showArchived`. */
export function isSectionFolded(id: string, folded: ReadonlySet<string>, showArchived: boolean, searching: boolean): boolean {
  if (id === 'archived') return !showArchived;
  return !searching && folded.has(id);
}

/** Toggles `id` in a stored list of folded section IDs. */
export function toggleFoldList(list: readonly string[], id: string): string[] {
  return list.includes(id) ? list.filter((x) => x !== id) : [...list, id];
}

/** Removes `id` from a stored list of folded section IDs, keeping the same array when absent. */
export function unfoldList(list: readonly string[], id: string): string[] {
  return list.includes(id) ? list.filter((x) => x !== id) : [...list];
}

const withTopHelpers = (rows: readonly SessionItem[], topHelpers: ReadonlyMap<string, readonly SessionItem[]>): SessionItem[] =>
  rows.flatMap((r) => [r, ...(topHelpers.get(r.session.key) ?? [])]);

const groupListRows = (g: GroupedSessions['pinned'][number]): ListRow[] => (g.lane ? [headItem(g.lane), ...g.rows] : g.rows);

/** The Sessions list's visible rows top to bottom, skipping any section that's folded. */
export function visibleSessionRows(parts: SessionSectionParts, folded: (id: string) => boolean): ListRow[] {
  const { orchestrators, listening, pinned, topHelpers, sections, groups, archived } = parts;
  const body: ListRow[] = groups
    ? [
        ...(folded('pinnedGroups') ? [] : groups.pinned.flatMap(groupListRows)),
        ...groups.sections.flatMap((s) => (folded(s.id) ? [] : s.groups.flatMap(groupListRows))),
      ]
    : sections.flatMap((s) => (folded(s.id) ? [] : s.rows));
  return [
    ...(folded('orchestrators') ? [] : orchestrators),
    ...(folded('listening') ? [] : withTopHelpers(listening, topHelpers)),
    ...(folded('pinned') ? [] : withTopHelpers(pinned, topHelpers)),
    ...body,
    ...(folded('archived') ? [] : archived),
  ];
}

/** Whether the Sessions view has any section to show (even if every section is folded), so the
 *  empty-list message only shows when there is genuinely nothing listed. */
export function hasSessionSections(parts: SessionSectionParts, showArchived: boolean): boolean {
  const { orchestrators, listening, pinned, sections, groups, archived } = parts;
  if (orchestrators.length > 0 || listening.length > 0 || pinned.length > 0) return true;
  if (groups ? groups.pinned.length > 0 || groups.sections.length > 0 : sections.length > 0) return true;
  return showArchived && archived.length > 0;
}

/** Which section holds row `key` (a session's `lane::key` or a worktree heading's `lane`), or
 *  null when it isn't in `parts`. */
export function sectionOfRow(key: string, parts: SessionSectionParts): string | null {
  const { orchestrators, listening, pinned, topHelpers, sections, groups, archived } = parts;
  if (orchestrators.some((r) => r.key === key)) return 'orchestrators';
  if (withTopHelpers(listening, topHelpers).some((r) => r.key === key)) return 'listening';
  if (withTopHelpers(pinned, topHelpers).some((r) => r.key === key)) return 'pinned';
  if (groups) {
    if (groups.pinned.some((g) => g.lane?.id === key || g.all.some((r) => r.key === key))) return 'pinnedGroups';
    for (const s of groups.sections) {
      if (s.groups.some((g) => g.lane?.id === key || g.all.some((r) => r.key === key))) return s.id;
    }
  } else {
    for (const s of sections) {
      if (s.rows.some((r) => r.key === key)) return s.id;
    }
  }
  if (archived.some((r) => r.key === key)) return 'archived';
  return null;
}

/** Which section `item` will land in, even before filters or age widen to list it (for `reveal`). */
export function sectionOfSession(
  item: SessionItem,
  all: readonly SessionItem[],
  o: { pinnedKeys: ReadonlySet<string>; sort: SessionSort; started: StartedPlace; group: SessionGroupBy; groupPins: readonly string[]; now: number },
): string {
  if (item.session.archived) return 'archived';
  if (item.session.orchestrator) return 'orchestrators';
  if (item.session.listening && hearingShows(item.session.status)) return 'listening';
  if (o.pinnedKeys.has(item.session.key)) return 'pinned';
  // Walk up to the session this one is listed under (if any), since nested rows share its section.
  const byKey = new Map(all.map((r) => [r.session.key, r]));
  let root = item;
  const seen = new Set<string>([item.session.key]);
  for (;;) {
    const up = listedUnder(root.session, o.started === 'under');
    const parent = up ? byKey.get(up) : undefined;
    if (!parent || seen.has(parent.session.key)) break;
    seen.add(parent.session.key);
    root = parent;
    if (root.session.listening && hearingShows(root.session.status)) return 'listening';
    if (o.pinnedKeys.has(root.session.key)) return 'pinned';
  }
  if (o.group !== 'none' && o.groupPins.includes(groupKeyOf(root.lane, o.group))) return 'pinnedGroups';
  return sessionSections([root], o.sort, o.now, o.started)[0]?.id ?? 'needs';
}

export interface SectionFolds {
  showArchived: boolean;
  setShowArchived: (show: boolean) => void;
  isFolded: (id: string) => boolean;
  toggle: (id: string) => void;
  unfold: (id: string) => void;
}

/** Fold state for the Sessions view's sections: persisted for ambient sections, ephemeral for
 *  urgent/day sections, and default-folded for Archived. */
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
    [setPersisted],
  );

  const unfold = useCallback(
    (id: string) => {
      if (id === 'archived') {
        setShowArchived(true);
      } else if (PERSISTED_SECTION_FOLDS.has(id)) {
        setPersisted((prev) => (prev.includes(id) ? unfoldList(prev, id) : prev));
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

  return { showArchived, setShowArchived, isFolded, toggle, unfold };
}
