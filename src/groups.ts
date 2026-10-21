// A list grouped by worktree or by repo: Group by, over Sessions, PRs and Todo. Each group heads
// its rows once, under the heading of its first row, with all of its rows after it in the list's
// order, so a heading counts only the rows under it for themselves. The sections a list folds
// away (Done, Merged & closed) group among themselves, and rows with no worktree or repo go under
// a heading of their own at the end of each section, rather than pulling every such row into one
// group under the first one's heading. Pinned groups (the worktrees and repos pinned, the same in
// every list) come first. A group lists `GROUP_SHOWN` rows and folds the rest until it's opened,
// but never one that waits on you. Pure, with tests (groups.test.ts); each list says how its rows
// group (`units`), where each goes (`placeOf`) and which never fold (`awake`).

import { repoOf } from './boards/rules';
import type { Lane } from './types';

/** Whether a list is grouped, and by what. */
export type ListGroupBy = 'none' | 'worktree' | 'repo';
export type GroupBy = Exclude<ListGroupBy, 'none'>;

/** Group by's buttons, each tooltip saying what the list puts under a heading (`sessions`). */
export function groupOptions(what: string): { id: ListGroupBy; label: string; icon: string; title: string }[] {
  return [
    { id: 'none', label: 'No groups', icon: 'layout-rows', title: `No groups: one list of ${what}` },
    { id: 'worktree', label: 'Worktree', icon: 'folder', title: `Group by worktree: each worktree once, saying its branch and PR, its app, whether new work can start there and where it stands against the default branch, with its ${what} under it` },
    { id: 'repo', label: 'Repo', icon: 'book', title: `Group by repo: each repo once, with its ${what} under it` },
  ];
}

/** How many rows a group lists (a session with what's nested under it counting as one) before it
 *  folds the rest away; more when more of them wait on you, which never fold. */
export const GROUP_SHOWN = 5;

/** A repo as its heading names it: its name, and its slug on GitHub where it has one. */
export interface RepoName {
  name: string;
  slug: string | null;
}

/** A worktree or a repo heading its rows. */
export interface ListGroup<R> {
  /** The worktree's id, or the repo's key (boards/rules.ts `repoOf`, or a PR's slug): what pins
   *  it. Empty for the rows with no worktree or repo. */
  key: string;
  by: GroupBy;
  /** The worktree heading a worktree's group. */
  lane: Lane | null;
  /** The repo heading a repo's group. */
  repo: RepoName | null;
  rows: R[];
  /** How many of its rows are folded away (`GROUP_SHOWN`). */
  folded: number;
  /** Opened past `GROUP_SHOWN`, so it can be folded again. */
  opened: boolean;
}

/** A section of the list with its groups in place of its rows. Its count is of its own rows: the
 *  ones in its groups that are under it for themselves, not for their group's first row. */
export type GroupedSection<S, R> = Omit<S, 'rows'> & { count: number; groups: ListGroup<R>[] };

export interface Grouped<S, R> {
  by: GroupBy;
  /** The groups pinned, in your order, after the list's pinned rows. */
  pinned: ListGroup<R>[];
  sections: GroupedSection<S, R>[];
}

/** The rows a section lists. */
type RowOf<S extends { rows: unknown[] }> = S['rows'][number];

/** Where a row goes: its group's key, empty for none, and what heads that group. */
export interface Place {
  key: string;
  lane: Lane | null;
  repo: RepoName | null;
}

export function repoNameOf(lane: Lane): RepoName {
  return { name: lane.repo, slug: lane.repoSlugNow ?? lane.repoSlug };
}

/** The group of a row in `lane`: the worktree's, or its repo's; none for a row in no worktree. */
export function placeIn(lane: Lane | null, by: GroupBy): Place {
  if (!lane) return { key: '', lane: null, repo: null };
  return by === 'worktree' ? { key: lane.id, lane, repo: null } : { key: repoOf(lane), lane: null, repo: repoNameOf(lane) };
}

export interface GroupOptions<R> {
  /** The groups pinned, in your order. */
  pinned: readonly string[];
  /** The project's worktrees, for the pinned groups with no row listed. */
  lanes: readonly Lane[];
  /** Whether a pinned group with no row listed shows. */
  empty: boolean;
  /** The groups opened past `GROUP_SHOWN`. */
  opened: ReadonlySet<string>;
  /** Every group lists all its rows: a search found them. */
  whole: boolean;
  /** A section's rows as the units a group lists or folds whole (a session and the rows nested
   *  under it); each row alone without it. */
  units?: (rows: R[]) => R[][];
  /** A unit that never folds away. */
  awake?: (unit: R[]) => boolean;
  /** The sections that group among themselves, apart from the rest: the ones the list folds. */
  apart?: (sectionId: string) => boolean;
}

/**
 * `sections`' rows grouped by worktree or by repo: each group once among the sections that group
 * together, under the heading of its first row, with all of its rows in their order; the groups
 * under a heading in the order of their first rows. Pinned groups leave those sections for
 * `pinned`, in your order, and show with no row listed while `empty`. A section `apart` groups its
 * own rows, and in every section the rows with no group (`placeOf` key empty) are a group of their
 * own, last. A group folds its rows after `GROUP_SHOWN` until it's opened, unless a search found
 * them (`whole`), but never an `awake` one, wherever it falls.
 */
export function groupRows<S extends { id: string; rows: unknown[] }>(sections: S[], by: GroupBy, placeOf: (r: RowOf<S>) => Place, o: GroupOptions<RowOf<S>>): Grouped<S, RowOf<S>> {
  type R = RowOf<S>;
  type Unit = { rows: R[]; from: string };
  type Found = { place: Place; units: Unit[]; at: number };
  const unitsOf = o.units ?? ((rows: R[]) => rows.map((r) => [r]));
  const awake = o.awake ?? (() => false);
  // The sections that group together, and each section's own: an apart one's, and the rows with
  // no group in any.
  const found = new Map<string, Found>();
  const own = sections.map(() => new Map<string, Found>());
  sections.forEach((s, at) => {
    for (const rows of unitsOf(s.rows as R[])) {
      const place = placeOf(rows[0]);
      const into = place.key === '' || o.apart?.(s.id) ? own[at] : found;
      const g = into.get(place.key) ?? { place, units: [], at };
      g.units.push({ rows, from: s.id });
      into.set(place.key, g);
    }
  });
  const build = (key: string, place: Place, units: Unit[]): ListGroup<R> => {
    const away = o.whole ? [] : units.filter((u, i) => i >= GROUP_SHOWN && !awake(u.rows));
    const opened = away.length > 0 && o.opened.has(key);
    const shown = opened ? units : units.filter((u) => !away.includes(u));
    const folded = opened ? 0 : away.reduce((n, u) => n + u.rows.length, 0);
    return { key, by, lane: place.lane, repo: place.repo, rows: shown.flatMap((u) => u.rows), folded, opened };
  };
  // A repo with no row listed is named by its main worktree, else by any of its worktrees.
  const emptyPlace = (key: string): Place | undefined => {
    const of = o.lanes.filter((l) => placeIn(l, by).key === key);
    const lane = of.find((l) => l.origin === 'main') ?? of[0];
    return lane ? placeIn(lane, by) : undefined;
  };
  const pinnedKeys = new Set(o.pinned);
  const pinned = o.pinned.flatMap((key) => {
    const g = found.get(key);
    if (g) return [build(key, g.place, g.units)];
    const place = o.empty ? emptyPlace(key) : undefined;
    return place ? [build(key, place, [])] : [];
  });
  const grouped = sections.map((s, at): GroupedSection<S, R> => {
    const mine = [...found].filter(([key, g]) => g.at === at && !pinnedKeys.has(key));
    // The rows with no group last, after the section's worktrees or repos.
    const apart = [...own[at]].sort(([a], [b]) => (a === '' ? 1 : 0) - (b === '' ? 1 : 0));
    const ownRows = (g: Found) => g.units.filter((u) => u.from === s.id).reduce((n, u) => n + u.rows.length, 0);
    const count = [...mine, ...apart].reduce((n, [, g]) => n + ownRows(g), 0);
    const { rows: _rows, ...rest } = s;
    return { ...rest, count, groups: [...mine, ...apart].map(([key, g]) => build(key, g.place, g.units)) };
  });
  return { by, pinned, sections: grouped.filter((s) => s.groups.length > 0) };
}

/** Every group of a grouped list, the pinned ones first, as the list draws them. */
export function groupsOf<S, R>(g: Grouped<S, R>): ListGroup<R>[] {
  return [...g.pinned, ...g.sections.flatMap((s) => s.groups)];
}
