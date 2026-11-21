// A list grouped by project, repo or worktree: Group by, over Sessions, PRs, Todo and Schedules.
// Non-apart sections merge into one unified list of groups so sorting orders rows underneath each
// group without reordering the groups themselves. Groups default to alphabetical order, remember
// user-dragged custom order per grouping kind (`order`), and put rows with no group last. Folded
// bottom sections (Done, Merged & closed, One-off, Finished) group among themselves in the same
// order. A group lists `GROUP_SHOWN` rows and folds the rest until it's opened, never hiding one
// that waits on you. Pure, with tests (groups.test.ts).

import { repoOf } from './boards/rules';
import { worktreeName } from './lanes';
import type { Lane } from './types';

/** Whether a list is grouped, and by what. */
export type ListGroupBy = 'none' | 'project' | 'repo' | 'worktree';
/** A group of a project's, a repo's or a worktree's rows. */
export type PlaceBy = Exclude<ListGroupBy, 'none'>;
/** What a grouped list's groups are: projects, repos or worktrees. */
export type GroupBy = PlaceBy;

/** Group by's buttons, each tooltip saying what the list puts under a heading (`sessions`). */
export function groupOptions(what: string): { id: ListGroupBy; label: string; icon: string; title: string }[] {
  return [
    { id: 'none', label: 'No groups', icon: 'layout-rows', title: `No groups: one list of ${what}` },
    { id: 'project', label: 'Project', icon: 'stack', title: `Group by project: each project once, with its ${what} under it` },
    { id: 'repo', label: 'Repo', icon: 'book', title: `Group by repo: each repo once, with its ${what} under it` },
    { id: 'worktree', label: 'Worktree', icon: 'folder', title: `Group by worktree: each worktree once, saying its branch and PR, its app, whether new work can start there and where it stands against the default branch, with its ${what} under it` },
  ];
}

/** What the heading over rows in no group says for `by`. */
export function noneLabel(by: GroupBy, worktreeNone: string): string {
  return by === 'project' ? 'No project' : by === 'repo' ? 'No repo' : worktreeNone;
}

/** How many rows a group lists (a session with what's nested under it counting as one) before it
 *  folds the rest away; more when more of them wait on you, which never fold. */
export const GROUP_SHOWN = 5;

/** A repo as its heading names it: its name, its slug on GitHub where it has one, and its
 *  project, said before it where the window lists every project. */
export interface RepoName {
  name: string;
  slug: string | null;
  project: string;
}

/** A project, repo or worktree heading its rows. */
export interface ListGroup<R> {
  /** The project's name, the repo's key (boards/rules.ts `repoOf`, or a PR's slug), or the
   *  worktree's id: what orders it. Empty for rows in none. */
  key: string;
  by: GroupBy;
  /** The worktree heading a worktree's group. */
  lane: Lane | null;
  /** The repo heading a repo's group. */
  repo: RepoName | null;
  /** The project heading a project's group. */
  project: string | null;
  rows: R[];
  /** Every row of it, the ones folded away too: where a row selected out of sight is. */
  all: R[];
  /** How many of its rows are folded away (`GROUP_SHOWN`). */
  folded: number;
  /** What identifies this group for expanding past `GROUP_SHOWN` and collapsing: includes the
   *  grouping kind (and section id when apart), so groups of different kinds or sections never
   *  collide. */
  fold: string;
  /** Opened past `GROUP_SHOWN`, so it can be folded again. */
  opened: boolean;
}

/** A folded bottom section (Done, Merged & closed, One-off, Finished) with its own groups. */
export type GroupedSection<S, R> = Omit<S, 'rows'> & { count: number; groups: ListGroup<R>[] };

export interface Grouped<S, R> {
  by: GroupBy;
  /** Every group of the main (non-apart) sections, in one unified list: custom order where
   *  reordered, alphabetical by default, with rows in no group last. Rows within each group keep
   *  the input sections' sorted order. */
  groups: ListGroup<R>[];
  /** Sections kept apart at the bottom (`apart`), each grouped among itself in the same order. */
  apart: GroupedSection<S, R>[];
}

/** The rows a section lists. */
type RowOf<S extends { rows: unknown[] }> = S['rows'][number];

/** Where a row goes: its group's key, empty for none, and what heads that group. */
export interface Place {
  key: string;
  lane: Lane | null;
  repo: RepoName | null;
  project: string | null;
}

export function repoNameOf(lane: Lane): RepoName {
  return { name: lane.repo, slug: lane.repoSlugNow ?? lane.repoSlug, project: lane.project };
}

/** The group of a row in `lane`: its worktree, its repo, or its project; none for a row in no
 *  worktree (or a chat with no project). */
export function placeIn(lane: Lane | null, by: PlaceBy): Place {
  if (!lane) return { key: '', lane: null, repo: null, project: null };
  if (by === 'worktree') return { key: lane.id, lane, repo: null, project: null };
  if (by === 'repo') return { key: repoOf(lane), lane: null, repo: repoNameOf(lane), project: null };
  const project = lane.project && !lane.startsChats ? lane.project : null;
  return project ? { key: project, lane: null, repo: null, project } : { key: '', lane: null, repo: null, project: null };
}

/** A group's display name for alphabetical ordering and drag labels. */
export function groupLabel(place: Pick<Place, 'lane' | 'repo' | 'project'>): string {
  if (place.lane) return worktreeName(place.lane);
  if (place.repo) return place.repo.name;
  return place.project ?? '';
}

/** Moves `key` to insertion point `index` among `shown`, the reorderable group keys currently
 *  displayed (excluding the empty-key `none` group), preserving any hidden keys already in `saved`. */
export function reorderGroup(shown: readonly string[], saved: readonly string[], key: string, index: number): string[] {
  const from = shown.indexOf(key);
  if (from < 0) return [...saved];
  const next = shown.filter((k) => k !== key);
  const to = Math.max(0, Math.min(next.length, index > from ? index - 1 : index));
  next.splice(to, 0, key);
  if (next.every((k, i) => k === shown[i])) return [...saved];
  const base = [...saved, ...shown.filter((k) => !saved.includes(k))];
  const slots = new Set(shown);
  let i = 0;
  return base.map((k) => (slots.has(k) ? next[i++] : k));
}

export interface GroupOptions<R> {
  /** Saved user order of group keys for this grouping kind; groups not named here sort
   *  alphabetically after them, so an empty list sorts all groups alphabetically. */
  order?: readonly string[];
  /** The groups opened past `GROUP_SHOWN`, by `ListGroup.fold`. */
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
 * `sections`' rows grouped by project, repo or worktree. Non-apart sections merge into one unified
 * `groups` list ordered by `order` then alphabetically (with rows in no group last), keeping each
 * group's rows in the input sections' active sort order so sorting never reorders the groups.
 * Sections matching `apart` group among themselves in the same group order. Each group folds rows
 * past `GROUP_SHOWN` unless `opened`, `whole`, or `awake`.
 */
export function groupRows<S extends { id: string; rows: unknown[] }>(sections: S[], by: GroupBy, placeOf: (r: RowOf<S>) => Place, o: GroupOptions<RowOf<S>>): Grouped<S, RowOf<S>> {
  type R = RowOf<S>;
  type Found = { place: Place; units: R[][] };
  const unitsOf = o.units ?? ((rows: R[]) => rows.map((r) => [r]));
  const awake = o.awake ?? (() => false);
  const main = new Map<string, Found>();
  const apartMaps = new Map<string, Map<string, Found>>();
  for (const s of sections) {
    const isApart = !!o.apart?.(s.id);
    const into = isApart ? (apartMaps.get(s.id) ?? apartMaps.set(s.id, new Map()).get(s.id)!) : main;
    for (const rows of unitsOf(s.rows as R[])) {
      const place = placeOf(rows[0]);
      const g = into.get(place.key) ?? { place, units: [] };
      g.units.push(rows);
      into.set(place.key, g);
    }
  }
  const rank = new Map((o.order ?? []).map((k, i) => [k, i]));
  const cmp = (x: string, y: string) => x.localeCompare(y, undefined, { sensitivity: 'base', numeric: true });
  const compare = ([ka, a]: [string, Found], [kb, b]: [string, Found]): number => {
    if (ka === '' || kb === '') return (ka === '' ? 1 : 0) - (kb === '' ? 1 : 0);
    const ra = rank.get(ka);
    const rb = rank.get(kb);
    if (ra !== undefined && rb !== undefined) return ra - rb;
    if (ra !== undefined) return -1;
    if (rb !== undefined) return 1;
    const pa = a.place.lane?.project ?? a.place.repo?.project ?? a.place.project ?? '';
    const pb = b.place.lane?.project ?? b.place.repo?.project ?? b.place.project ?? '';
    const repoA = a.place.lane?.repo ?? a.place.repo?.name ?? '';
    const repoB = b.place.lane?.repo ?? b.place.repo?.name ?? '';
    return cmp(groupLabel(a.place), groupLabel(b.place)) || cmp(pa, pb) || cmp(repoA, repoB) || ka.localeCompare(kb);
  };
  const build = (key: string, place: Place, units: R[][], fold: string): ListGroup<R> => {
    const away = o.whole ? [] : units.filter((u, i) => i >= GROUP_SHOWN && !awake(u));
    const opened = away.length > 0 && o.opened.has(fold);
    const shown = opened ? units : units.filter((u) => !away.includes(u));
    const folded = opened ? 0 : away.reduce((n, u) => n + u.length, 0);
    return { key, by, lane: place.lane, repo: place.repo, project: place.project, rows: shown.flat(), all: units.flat(), folded, fold, opened };
  };
  const groups = [...main].sort(compare).map(([key, g]) => build(key, g.place, g.units, key ? `${by}/${key}` : `main/${by}/`));
  const apart: GroupedSection<S, R>[] = [];
  for (const s of sections) {
    const map = apartMaps.get(s.id);
    if (!map || map.size === 0) continue;
    const secGroups = [...map].sort(compare).map(([key, g]) => build(key, g.place, g.units, `${s.id}/${by}/${key}`));
    const count = secGroups.reduce((n, g) => n + g.all.length, 0);
    const { rows: _rows, ...rest } = s;
    apart.push({ ...rest, count, groups: secGroups });
  }
  return { by, groups, apart };
}

/** Every group of a grouped list, main groups then apart sections' groups, as the list draws them. */
export function groupsOf<S, R>(g: Grouped<S, R>): ListGroup<R>[] {
  return [...g.groups, ...g.apart.flatMap((s) => s.groups)];
}
