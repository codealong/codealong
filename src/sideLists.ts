// What the main window's sidebar lists, and what says which filters narrow the view (App.tsx):
// the views with their counts, the project's boards under the Board view, the statuses, the facets
// a view offers, the names of the filters on for the collapsed rail's badge, the same filters as
// chips on the list's strip (ui/ListBar.tsx `FilterChips`), and the name a view is offered when
// it's saved. Pure functions of what the window has, with tests (sideLists.test.ts).

import { FACET_WORD, facetChips, facetsIn, type FacetId, type FacetOption, type Facets } from './boardLists';
import { ruleWords, type BoardDoc } from './boards/model';
import type { DeviceFilter } from './devices';
import type { Filter } from './lanes';
import { VIEW_CHORDS } from './mainKeys';
import { AGES, VIEWS, type Age, type View } from './mainViews';
import { chordKeys } from './platform';
import { ALL_PROJECTS } from './quick';
import { suggestName } from './savedViews';
import type { Label } from './types';
import type { BoardLayout } from './ui/BoardBar';
import type { ListFilter } from './ui/ListBar';
import type { SideBoard, SideStatus, SideView } from './ui/Sidebar';

const NO_PICKS: readonly string[] = [];

/** What the sidebar counts beside each view. */
export interface ViewCounts {
  sessions: number;
  todos: number;
  /** The project's schedules that aren't paused or finished. */
  schedules: number;
  /** The open PRs, and whether GitHub's are still on their way. */
  prs: number;
  prsComing: boolean;
  /** The PRs that want your review. */
  requested: number;
}

/** The views as the sidebar lists them, with their counts: the Board view once, for Kanban and
 *  Canvas both, with the keys of the layout a click on it shows (`layoutShown`: the one showing,
 *  else the last). */
export function sideViewsOf(views: readonly { id: View; label: string; icon: string }[], layoutShown: BoardLayout, counts: ViewCounts): SideView[] {
  return views
    .filter((v) => v.id !== 'canvas')
    .map((v) => {
      const board = v.id === 'kanban';
      return {
        id: board ? 'board' : v.id,
        label: board ? 'Board' : v.label,
        icon: v.icon,
        // The Board's keys are those of the layout a click on it shows: the one showing, else the last.
        kbd: chordKeys(VIEW_CHORDS[board ? layoutShown : v.id]),
        count:
          v.id === 'sessions'
            ? counts.sessions
            : v.id === 'todo'
              ? counts.todos
              : v.id === 'schedules'
                ? counts.schedules
                : board
                  ? ''
                  : counts.prsComing && counts.prs === 0
                    ? ''
                    : counts.prs,
        badge: v.id === 'prs' && counts.requested > 0 ? counts.requested : undefined,
        badgeTitle: v.id === 'prs' ? `${counts.requested} want your review` : undefined,
      };
    });
}

/** The project's boards under the Board view (under All projects, every project's, each saying
 *  its project). The one the view shows has the view's keys beside it, which go back to it, and
 *  its count while it shows. */
export function sideBoardsOf(
  boards: readonly BoardDoc[],
  has: { current: string; labels: Label[]; shown: string | undefined; showing: boolean; count: number | string; layoutShown: BoardLayout },
): SideBoard[] {
  return boards.map((b) => ({
    id: b.id,
    name: b.name,
    project: b.project !== has.current ? b.project : undefined,
    title: `${b.project !== has.current ? `In ${b.project}: ` : ''}${ruleWords(b.rule, has.labels)}`,
    count: has.showing && b.id === has.shown ? has.count : undefined,
    kbd: b.id === has.shown ? chordKeys(VIEW_CHORDS[has.layoutShown]) : undefined,
  }));
}

/** The statuses the sidebar narrows a view to, with how many of each there are. */
export function sideStatusesOf(counts: { needs: number; unread: number; working: number; review: number; work: number }): SideStatus[] {
  return [
    { id: 'needs', label: 'Needs you', glyph: 'needs', count: counts.needs, tone: 'attn' },
    { id: 'unread', label: 'Your turn', glyph: 'unread', count: counts.unread },
    { id: 'working', label: 'Working', glyph: 'working', count: counts.working },
    { id: 'review', label: 'In review', icon: 'pr', count: counts.review },
    { id: 'work', label: 'PR needs work', glyph: 'error', count: counts.work, tone: 'alert', title: 'Open PRs with failing checks or changes requested' },
  ];
}

/** Whether the sidebar's age filter narrows the view: not Todo, the canvas or Schedules. */
export function ageAppliesIn(view: View): boolean {
  return view !== 'todo' && view !== 'canvas' && view !== 'schedules';
}

/** A facet the view offers, with its options and the picks among them. */
export interface SideFacet {
  id: FacetId;
  label: string;
  options: FacetOption[];
  picked: readonly string[];
}

/** The facets the view offers, and the picks among them that narrow it now. */
export function sideFacetsOf(view: View, options: Record<FacetId, FacetOption[]>, facets: Facets): SideFacet[] {
  return facetsIn(view).map((id) => ({ id, label: FACET_WORD[id], options: options[id], picked: facets[id] ?? NO_PICKS }));
}

/** The picks that narrow the view, by their names. */
export function pickNamesOf(facets: readonly SideFacet[]): string[] {
  return facets.flatMap((f) => f.picked.map((p) => f.options.find((o) => o.id === p)?.label ?? p));
}

/** The filters on: the devices, the age where the view goes by it (`ageAppliesIn`), the
 *  statuses, and the facets with their picks. */
export interface FiltersOn {
  devices: DeviceFilter;
  /** A device's name by its id, where it's known. */
  deviceName: (id: string) => string | null;
  ageApplies: boolean;
  age: Age;
  statuses: readonly Filter[];
  sideStatuses: readonly SideStatus[];
  facets: SideFacet[];
}

/** What narrows the view now, in words, for the collapsed rail's badge: the age, the statuses,
 *  the facets' picks. */
export function sideFiltersOf(on: FiltersOn, pickNames: readonly string[]): string[] {
  return [
    on.ageApplies && on.age !== 'all' ? (AGES.find((a) => a.id === on.age)?.title.replace(/^Active in the /, '') ?? on.age) : null,
    ...on.statuses.map((f) => on.sideStatuses.find((x) => x.id === f)?.label ?? f),
    ...pickNames,
  ].filter((x): x is string => !!x);
}

/** How each filter is taken off. */
export interface FilterClears {
  device: (id: string) => void;
  age: () => void;
  status: (f: Filter) => void;
  facet: (facet: FacetId, pick: string) => void;
}

/** The same filters as chips on the list's strip (ListBar.tsx `FilterChips`), each with the way
 *  to take it off, so what narrows the list shows with the sidebar folded away. */
export function listFiltersOf(on: FiltersOn, clear: FilterClears): ListFilter[] {
  return [
    ...on.devices.map((id) => {
      const name = on.deviceName(id) ?? id;
      return { key: `device:${id}`, label: name, name: `what runs on ${name}`, clear: () => clear.device(id) };
    }),
    ...(on.ageApplies && on.age !== 'all'
      ? [{ key: 'age', label: (AGES.find((a) => a.id === on.age)?.title ?? on.age).replace(/^Active in the l/, 'L'), name: (AGES.find((a) => a.id === on.age)?.title ?? on.age).toLowerCase(), clear: () => clear.age() }]
      : []),
    ...on.statuses.map((f) => {
      const label = on.sideStatuses.find((x) => x.id === f)?.label ?? f;
      return { key: `status:${f}`, label, name: label.toLowerCase(), clear: () => clear.status(f) };
    }),
    ...facetChips(on.facets).map((c) => ({ key: `${c.facet}:${c.pick}`, label: c.label, name: c.name, title: c.title, clear: () => clear.facet(c.facet, c.pick) })),
  ];
}

const either = (words: (string | null | undefined)[]) => words.filter(Boolean).join(' + ') || null;

/** The name a view is offered as it's saved: the view, then what narrows it. */
export function viewSuggestionOf(view: View, current: string, on: FiltersOn, pickNames: string[]): string {
  return suggestName([
    VIEWS.find((v) => v.id === view)?.label,
    current === ALL_PROJECTS ? null : current,
    either(on.devices.map(on.deviceName)),
    either(on.statuses.map((f) => on.sideStatuses.find((x) => x.id === f)?.label)),
    either(pickNames),
    on.ageApplies && on.age !== 'all' ? AGES.find((a) => a.id === on.age)?.label : null,
  ]);
}
