// What the main window's views list, worked out from the board, the sidebar's filters and the
// search: the project's worktrees in their bays, its sessions with the pinned ones first, its
// PRs and todos in their sections, and the counts the sidebar and the switcher show. The rules
// are pure functions with tests (boardLists.test.ts); `useBoardLists` keeps each list until
// what it's made from changes, so rows and memos downstream see the same lists between pushes.

import { useCallback, useMemo } from 'react';
import type { DeviceFilter } from './devices';
import { byDevice, needsOn, withDevices, workingOn } from './devices';
import { AGENT_BAY, buildBays, filterByAge, inboxCounts, keptCopy, narrowed, prNeedsWork, rowItems, sessionHomes, worktreeRow, type BoardRow, type Filter, type LaneCopies, type Placed, type WorktreeRow } from './lanes';
import type { Pins } from './pins';
import { ALL_PROJECTS, inProject } from './quick';
import { todoIndex, todoInFilter, todoInQuery, todoItem, todoSections, type TodoItem } from './todos';
import type { Bay, Board, DevicesView, Label, Lane, PrItem, ProjectSummary, Session, Todo } from './types';
import type { ActionCounts } from './ui/ProjectSwitcher';
import { prIsOpen, prPlaces, prSections, waitsOnReview, type PrRowItem } from './ui/PrsView';
import { sortSessions, waitingQueue, type SessionItem, type SessionSort } from './ui/SessionsView';

const ELSEWHERE = 'Elsewhere';

/** A row any of the list views shows. */
export type ListRow = BoardRow | PrRowItem | TodoItem;

/** This device's projects, then any only another device has: its worktrees go into this device's
 *  projects by repo (peers.rs), and the rest keep that device's own project, listed here too,
 *  before Elsewhere. */
export function projectsOf(here: ProjectSummary[], deviceLanes: Lane[]): ProjectSummary[] {
  const names = new Set(here.map((p) => p.name));
  const elsewhere: Map<string, number> = new Map();
  for (const l of deviceLanes) {
    if (!names.has(l.project) && !l.hidden) elsewhere.set(l.project, (elsewhere.get(l.project) ?? 0) + 1);
  }
  if (elsewhere.size === 0) return here;
  const extra = [...elsewhere].map(([name, lanes]) => ({ name, lanes, repos: [] }));
  const last = here.length > 0 && here[here.length - 1].name === ELSEWHERE ? here.length - 1 : here.length;
  return [...here.slice(0, last), ...extra, ...here.slice(last)];
}

/** The project the window shows: the one picked while the board lists it, else the first but
 *  Elsewhere, else all of them. The board lists no projects until it's read, so the one you left
 *  it on holds until then. */
export function currentProject(picked: string | null, projects: { name: string }[], loading: boolean): string {
  const known = picked === ALL_PROJECTS || projects.some((p) => p.name === picked) || (loading && !!picked);
  return known && picked ? picked : (projects.find((p) => p.name !== ELSEWHERE)?.name ?? ALL_PROJECTS);
}

/** Search on the board: the worktree, its PR and the accounts and titles of its sessions. */
export function laneInQuery(lane: Lane, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  const hay = [lane.callsign, lane.repo, lane.branch ?? '', lane.headline, lane.path, lane.pr ? `#${lane.pr.number} ${lane.pr.title}` : '', ...lane.sessions.map((s) => `${s.account} ${s.title}`)];
  return hay.some((h) => h.toLowerCase().includes(needle));
}

/** Search in the Sessions view: the session, then its worktree and PR. */
export function sessionInQuery(r: SessionItem, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  const { lane, session: s } = r;
  const hay = [s.title, s.account, s.excerpt ?? '', lane.callsign, lane.repo, lane.branch ?? '', lane.pr ? `#${lane.pr.number} ${lane.pr.title}` : ''];
  return hay.some((h) => h.toLowerCase().includes(needle));
}

/** The status filters, applied to one session: any of them. */
export function sessionInFilter(r: SessionItem, fs: readonly Filter[]): boolean {
  return fs.length === 0 || fs.some((f) => sessionIn(r, f));
}

export function sessionIn(r: SessionItem, f: Filter): boolean {
  const pr = r.lane.pr;
  switch (f) {
    case 'needs':
      return r.session.status === 'needs' || r.session.status === 'error';
    case 'working':
    case 'unread':
      return r.session.status === f;
    case 'review':
      return !!pr && pr.state !== 'merged' && pr.state !== 'closed';
    case 'work':
      return prNeedsWork(r.lane.pr);
    default:
      return r.lane.bay === f;
  }
}

/** Search in the PRs view: number, title, author, branch, repo. */
export function prInQuery(p: PrItem, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase().replace(/^#/, '');
  return [String(p.number), p.title, p.author ?? '', p.head, p.repoSlug].some((h) => h.toLowerCase().includes(needle));
}

/** The status filters, applied to a PR and the sessions that worked on it: any of them. */
export function prInFilter(r: PrRowItem, fs: readonly Filter[]): boolean {
  return fs.length === 0 || fs.some((f) => prIn(r, f));
}

export function prIn(r: PrRowItem, f: Filter): boolean {
  const p = r.pr;
  switch (f) {
    case 'needs':
    case 'working':
    case 'unread':
      // As on the board, a session from another lane doesn't lend its status: one agent that
      // made many worktrees would otherwise put all of their PRs in Working.
      return r.sessions.some(({ session }) => !session.visitor && AGENT_BAY[session.status] === f);
    case 'review':
      return prIsOpen(p);
    case 'work':
      return prNeedsWork(p);
    default:
      return false;
  }
}

const UNARCHIVED: LaneCopies = new WeakMap();

/** Every worktree with its archived sessions left out: they leave every view but the Archived list.
 *  A worktree that's the same object as at the last push gives back the same copy (`keptCopy`). */
export function unarchived(lanes: Lane[]): Lane[] {
  return lanes.map((l) => (l.sessions.some((s) => s.archived) ? keptCopy(l, UNARCHIVED, '', () => ({ ...l, sessions: l.sessions.filter((s) => !s.archived) })) : l));
}

/** The project's archived sessions, the latest first, for the Archived list: with a search, the
 *  ones it finds, so the list says how many archived sessions match and shows them. */
export function archivedIn(lanes: Lane[], project: string, query = ''): SessionItem[] {
  return lanes
    .filter((l) => inProject(l, project))
    .flatMap((lane) => lane.sessions.filter((s) => !s.visitor && s.archived).map((s): SessionItem => ({ kind: 'agent', key: `${lane.id}::${s.key}`, lane, session: s })))
    .filter((r) => sessionInQuery(r, query))
    .sort((a, b) => b.session.lastActivity - a.session.lastActivity);
}

/** Each session once, in the worktree it runs in: the Sessions view's rows. */
export function sessionItemsOf(lanes: Lane[]): SessionItem[] {
  return lanes.flatMap((lane) => lane.sessions.filter((s) => !s.visitor).map((s): SessionItem => ({ kind: 'agent', key: `${lane.id}::${s.key}`, lane, session: s })));
}

/** What the age filter lets through: sessions active since the cutoff, and pinned ones. */
function recentSession(r: SessionItem, cutoff: number, pinned: Set<string>): boolean {
  return r.session.lastActivity >= cutoff || pinned.has(r.session.key);
}

/** The sessions the Sessions view shows: recent or pinned, matching the search and the filters. */
export function shownSessionsOf(items: SessionItem[], cutoff: number, pinned: Set<string>, query: string, filter: readonly Filter[]): SessionItem[] {
  return items.filter((r) => recentSession(r, cutoff, pinned) && sessionInQuery(r, query) && sessionInFilter(r, filter));
}

/** How many sessions the age filter lets through, whatever the search and the filters: the sidebar's count. */
export function recentSessionCount(items: SessionItem[], cutoff: number, pinned: Set<string>): number {
  return items.filter((r) => recentSession(r, cutoff, pinned)).length;
}

/** The rows that are pinned, in the pins' order. */
export function inPinOrder<T>(rows: T[], keys: string[], keyOf: (r: T) => string): T[] {
  const byKey = new Map(rows.map((r) => [keyOf(r), r]));
  return keys.flatMap((k) => byKey.get(k) ?? []);
}

/** Pinned worktrees that match the search and the status filter, in your order. Each shows at the
 *  top of the board and leaves the bays below. */
export function pinnedLaneRowsOf(lanes: Lane[], keys: string[], query: string, filter: readonly Filter[], since: string): WorktreeRow[] {
  const byId = new Map(lanes.map((l) => [l.id, l]));
  return keys.flatMap((id): WorktreeRow[] => {
    const lane = byId.get(id);
    const shown = lane && laneInQuery(lane, query) ? narrowed(lane, filter, since) : null;
    return shown ? [worktreeRow(shown)] : [];
  });
}

/** Every other worktree once, in the bay its status puts it in. The status filter narrows each to
 *  what it's about, which can move it to that bay. */
export function bayRowsOf(lanes: Lane[], query: string, pinned: Set<string>, filter: readonly Filter[], since: string): { bay: Bay; rows: WorktreeRow[] }[] {
  return buildBays(lanes.flatMap((l) => (laneInQuery(l, query) && !pinned.has(l.id) ? (narrowed(l, filter, since) ?? []) : [])));
}

/** The bays with parked worktrees folded away, unless they're asked for or searched. */
export function folded(bays: { bay: Bay; rows: WorktreeRow[] }[], foldParked: boolean): { bay: Bay; rows: WorktreeRow[] }[] {
  return bays.map((b) => ({ ...b, rows: b.rows.filter((r) => !foldParked || !r.lane.parked) }));
}

/** Each session by its worktree's id and its key, in the worktree it's listed under. */
export function sessionIndexOf(lanes: Lane[]): Map<string, { lane: Lane; session: Session }> {
  const m = new Map<string, { lane: Lane; session: Session }>();
  for (const lane of lanes) for (const session of lane.sessions) m.set(`${lane.id}::${session.key}`, { lane, session });
  return m;
}

/** A PR as the PRs view lists it: with the worktrees that hold it and the sessions that worked on
 *  it. A session is looked up in the worktree it's listed under, which for one that made or
 *  worked in a worktree isn't its own. */
export function prRowOf(pr: PrItem, laneById: Map<string, Lane>, sessionIndex: Map<string, { lane: Lane; session: Session }>): PrRowItem {
  const holding = pr.lanes.flatMap((id) => laneById.get(id) ?? []);
  const sessions = pr.sessions.flatMap((x) => sessionIndex.get(`${x.lane}::${x.key}`) ?? []);
  return { kind: 'pr', key: `pr:${pr.key}`, pr, lane: holding[0] ?? null, sessions, places: prPlaces(holding, sessions) };
}

/** The PRs the PRs view shows: updated since the cutoff or pinned, matching the search and the filters. */
export function prRowsOf(prs: PrItem[], cutoff: number, pinned: Set<string>, query: string, filter: readonly Filter[], laneById: Map<string, Lane>, sessionIndex: Map<string, { lane: Lane; session: Session }>): PrRowItem[] {
  return prs
    .filter((p) => (p.updatedAt >= cutoff || pinned.has(p.key)) && prInQuery(p, query))
    .map((p) => prRowOf(p, laneById, sessionIndex))
    .filter((r) => prInFilter(r, filter));
}

/** Whether a PR is listed under Merged & closed, which the PRs view keeps folded until it's
 *  opened: one that's merged or closed and not pinned, since pinned PRs lead the view whatever
 *  their state. A window asked to show such a PR opens that section, or the panel would show the
 *  view's first PR in its place. */
export function prInDone(pr: PrItem, pinned: ReadonlySet<string>): boolean {
  return !prIsOpen(pr) && !pinned.has(pr.key);
}

/** The age filter a window asked to show a PR widens to, from `current`: the narrowest of `ages`
 *  (narrowest first, a span of 0 for no limit) that lists a PR last updated when this one was,
 *  as `prRowsOf` goes by. Null when the filter lists it already, or it's pinned, which lists it
 *  whatever its age. Without it the PR isn't listed and can't be selected, and the panel would
 *  show the view's first PR in its place, as it did for one under Merged & closed (`prInDone`). */
export function ageListingPr<A extends string>(pr: PrItem, pinned: ReadonlySet<string>, ages: readonly { id: A; ms: number }[], current: A, now: number): A | null {
  return ageListing(pr.updatedAt, pinned.has(pr.key), ages, current, now);
}

/** The age filter a window asked to show a session widens to, as `ageListingPr` has it for a PR:
 *  the narrowest that lists a session last active when this one was, as the Sessions view goes by
 *  (`shownSessionsOf`). Null when the filter lists it already, or it's pinned. Without it the
 *  session isn't listed, and Sessions would select another of its worktree's, or its first. */
export function ageListingSession<A extends string>(s: Session, pinned: ReadonlySet<string>, ages: readonly { id: A; ms: number }[], current: A, now: number): A | null {
  return ageListing(s.lastActivity, pinned.has(s.key), ages, current, now);
}

/** The narrowest of `ages` that lists what was last active `at`, when `current` doesn't. */
function ageListing<A extends string>(at: number, pinned: boolean, ages: readonly { id: A; ms: number }[], current: A, now: number): A | null {
  const lists = (ms: number) => ms === 0 || at >= now - ms;
  const shown = ages.find((a) => a.id === current);
  if (pinned || !shown || lists(shown.ms)) return null;
  return ages.find((a) => lists(a.ms))?.id ?? null;
}

/** Needs you and Your turn per project, for the switcher's badge and menu. */
export function projectActions(lanes: Lane[], projects: { name: string }[], cutoff: number, pinned: Set<string>, since: string): Record<string, ActionCounts> {
  const out: Record<string, ActionCounts> = {};
  for (const name of [...projects.map((p) => p.name), ALL_PROJECTS]) {
    const { needs, unread } = inboxCounts(filterByAge(lanes.filter((l) => inProject(l, name)), cutoff, pinned, since));
    out[name] = { needs, unread };
  }
  return out;
}

/** How many sessions need you, or are working, on each device, for the sidebar's devices. */
export function deviceCountsOf(lanes: Lane[], devices: { short: string }[], count: (lanes: Lane[], short: string | null) => number): Map<string | null, number> {
  return new Map<string | null, number>([null, ...devices.map((d) => d.short)].map((m) => [m, count(lanes, m)]));
}

export interface ListsInput {
  board: Board | null;
  /** Other devices' worktrees, with your changes on top. */
  deviceLanes: Lane[];
  devices: DevicesView | null;
  deviceFilter: DeviceFilter;
  groupByDevice: boolean;
  /** The project picked, before it's checked against the projects there are. */
  project: string | null;
  loading: boolean;
  /** The age filter: active since `cutoff` (0 for any time), and how a worktree with nothing
   *  left after it says so. */
  cutoff: number;
  since: string;
  now: number;
  query: string;
  filter: readonly Filter[];
  sessionPins: Pins;
  lanePins: Pins;
  prPins: Pins;
  sessionSort: SessionSort;
  showParked: boolean;
  showArchived: boolean;
  showDonePrs: boolean;
  todos: Todo[];
  labels: Label[];
  labelFilter: string | null;
  showDoneTodos: boolean;
}

/** The lists, each kept while what it's made from stays the same. */
export function useBoardLists(input: ListsInput) {
  const { board, deviceLanes, devices, deviceFilter, groupByDevice, project, loading, cutoff, since, now, query, filter, sessionPins, lanePins, prPins, sessionSort, showParked, showArchived, showDonePrs, todos, labels, labelFilter, showDoneTodos } = input;

  const projects = useMemo(() => projectsOf(board?.projects ?? [], deviceLanes), [board?.projects, deviceLanes]);
  const current = currentProject(project, projects, loading);

  // Archived sessions leave every view but the Archived list.
  const rawLanes = useMemo(() => withDevices(board?.lanes ?? [], deviceLanes, deviceFilter), [board?.lanes, deviceLanes, deviceFilter]);
  // What needs you on each device, for the sidebar, whichever device the views show.
  const deviceNeeds = useMemo(() => deviceCountsOf(withDevices(board?.lanes ?? [], deviceLanes, []), devices?.devices ?? [], needsOn), [board?.lanes, deviceLanes, devices?.devices]);
  const deviceWorking = useMemo(() => deviceCountsOf(withDevices(board?.lanes ?? [], deviceLanes, []), devices?.devices ?? [], workingOn), [board?.lanes, deviceLanes, devices?.devices]);
  const lanes = useMemo(() => unarchived(rawLanes), [rawLanes]);
  const projectLanes = useMemo(() => lanes.filter((l) => inProject(l, current)), [lanes, current]);
  const archivedItems = useMemo(() => archivedIn(rawLanes, current, query), [rawLanes, current, query]);
  // Worktrees active since the cutoff and pinned ones, each showing only its sessions active
  // since then.
  const recentLanes = useMemo(() => filterByAge(projectLanes, cutoff, lanePins.set, since), [projectLanes, cutoff, lanePins.set, since]);

  // The Sessions view: each session once, in the worktree it runs in, pinned ones first.
  const sessionItems = useMemo(() => sessionItemsOf(projectLanes), [projectLanes]);
  const shownSessions = useMemo(() => shownSessionsOf(sessionItems, cutoff, sessionPins.set, query, filter), [sessionItems, cutoff, sessionPins.set, query, filter]);
  const shownSessionCount = useMemo(() => recentSessionCount(sessionItems, cutoff, sessionPins.set), [sessionItems, cutoff, sessionPins.set]);
  const pinnedRows = useMemo(() => inPinOrder(shownSessions, sessionPins.keys, (r) => r.session.key), [shownSessions, sessionPins.keys]);
  const sessionList = useMemo(() => sortSessions(shownSessions.filter((r) => !sessionPins.set.has(r.session.key)), sessionSort, now), [shownSessions, sessionPins.set, sessionSort, now]);
  const sessionRows = useMemo<ListRow[]>(() => [...pinnedRows, ...sessionList, ...(showArchived ? archivedItems : [])], [pinnedRows, sessionList, showArchived, archivedItems]);
  // What waits on you, for working through it from a session's panel (useWaitingQueue.ts).
  const waitingRows = useMemo(() => waitingQueue(shownSessions), [shownSessions]);

  // The board: pinned worktrees, then every other worktree once in its bay.
  const pinnedLaneRows = useMemo(() => pinnedLaneRowsOf(recentLanes, lanePins.keys, query, filter, since), [recentLanes, lanePins.keys, query, filter, since]);
  const bayRows = useCallback((ls: Lane[]) => bayRowsOf(ls, query, lanePins.set, filter, since), [query, lanePins.set, filter, since]);
  const bays = useMemo(() => bayRows(recentLanes), [bayRows, recentLanes]);
  // Grouped by device, each device's worktrees get bays of their own: this device's first.
  const grouped = groupByDevice && deviceFilter.length !== 1 && (devices?.enabled ?? false) && (devices?.lanes.length ?? 0) > 0;
  const deviceGroups = useMemo(() => (grouped ? byDevice(recentLanes, devices).map((g) => ({ short: g.short, bays: bayRows(g.lanes) })) : null), [grouped, recentLanes, devices, bayRows]);
  const foldParked = !showParked && !query;
  const fold = useCallback((bs: typeof bays) => folded(bs, foldParked), [foldParked]);
  const shownBays = useMemo(() => fold(bays), [fold, bays]);
  const shownGroups = useMemo(() => (deviceGroups ?? [{ short: undefined as string | null | undefined, bays }]).map((g) => ({ ...g, shown: fold(g.bays) })), [deviceGroups, bays, fold]);
  // What you can select on the board, top to bottom: agents' lines, and worktrees without any.
  const worktreeItems = useMemo(() => [...pinnedLaneRows, ...shownGroups.flatMap((g) => g.shown.flatMap((b) => b.rows))].flatMap(rowItems), [pinnedLaneRows, shownGroups]);

  // The PRs view: the project's PRs with the worktrees that hold them and the sessions that
  // worked on them.
  const sessionIndex = useMemo(() => sessionIndexOf(lanes), [lanes]);
  const laneById = useMemo(() => new Map(lanes.map((l) => [l.id, l])), [lanes]);
  const prRow = useCallback((pr: PrItem): PrRowItem => prRowOf(pr, laneById, sessionIndex), [laneById, sessionIndex]);
  const projectPrs = useMemo(() => (board?.prs ?? []).filter((p) => current === ALL_PROJECTS || p.project === current), [board?.prs, current]);
  const prRows = useMemo(() => prRowsOf(projectPrs, cutoff, prPins.set, query, filter, laneById, sessionIndex), [projectPrs, cutoff, query, filter, laneById, sessionIndex, prPins.set]);
  // Pinned PRs lead the PRs view in your order, whatever their age, and leave its sections.
  const pinnedPrRows = useMemo(() => inPinOrder(prRows, prPins.keys, (r) => r.pr.key), [prRows, prPins.keys]);
  const prSecs = useMemo(() => prSections(prRows.filter((r) => !prPins.set.has(r.pr.key))), [prRows, prPins.set]);
  const prVisible = useMemo<ListRow[]>(() => [...pinnedPrRows, ...prSecs.flatMap((s) => (s.id === 'done' && !showDonePrs ? [] : s.rows))], [pinnedPrRows, prSecs, showDonePrs]);
  const openPrCount = useMemo(() => projectPrs.filter((p) => prIsOpen(p) && p.updatedAt >= cutoff).length, [projectPrs, cutoff]);
  const requestedCount = useMemo(() => projectPrs.filter(waitsOnReview).length, [projectPrs]);

  // The Todo view: the project's todos, with the worktrees and sessions they're linked to.
  const homes = useMemo<Map<string, Placed>>(() => sessionHomes(lanes), [lanes]);
  const labelById = useMemo(() => new Map(labels.map((l) => [l.id, l])), [labels]);
  const projectTodos = useMemo(() => todos.filter((t) => current === ALL_PROJECTS || t.project === current), [todos, current]);
  const todoItems = useMemo(() => projectTodos.map((t) => todoItem(t, laneById, homes, labelById)), [projectTodos, laneById, homes, labelById]);
  const todoRows = useMemo(() => todoItems.filter((r) => todoInQuery(r, query) && todoInFilter(r, filter) && (!labelFilter || r.todo.labels.includes(labelFilter))), [todoItems, query, filter, labelFilter]);
  const todoSecs = useMemo(() => todoSections(todoRows, todos), [todoRows, todos]);
  const todoVisible = useMemo<ListRow[]>(() => todoSecs.flatMap((s) => (s.id === 'done' && !showDoneTodos ? [] : s.rows)), [todoSecs, showDoneTodos]);
  const openTodoCount = useMemo(() => projectTodos.filter((t) => t.status !== 'done').length, [projectTodos]);
  // Which todos each session and worktree is on, for their chips.
  const todoLinks = useMemo(() => todoIndex(todos, homes), [todos, homes]);
  // Which todos each session is linked to depends on the todos alone, so rows that ask keep one
  // function between pushes and needn't render again.
  const sessionTodos = useMemo(() => todoIndex(todos, new Map()).bySession, [todos]);
  const todosFor = useCallback((k: string) => sessionTodos.get(k), [sessionTodos]);

  // The sidebar's and the switcher's counts.
  const counts = useMemo(() => inboxCounts(recentLanes), [recentLanes]);
  const totalLanes = useMemo(() => lanes.filter((l) => !l.hidden).length, [lanes]);
  const actions = useMemo(() => projectActions(lanes, projects, cutoff, lanePins.set, since), [lanes, projects, cutoff, lanePins.set, since]);

  return {
    projects,
    current,
    lanes,
    projectLanes,
    deviceNeeds,
    deviceWorking,
    archivedItems,
    recentLanes,
    shownSessions,
    shownSessionCount,
    pinnedRows,
    sessionList,
    sessionRows,
    waitingRows,
    pinnedLaneRows,
    bays,
    shownBays,
    shownGroups,
    foldParked,
    worktreeItems,
    laneById,
    prRowOf: prRow,
    prRows,
    pinnedPrRows,
    prSecs,
    prVisible,
    openPrCount,
    requestedCount,
    homes,
    labelById,
    projectTodos,
    todoItems,
    todoSecs,
    todoVisible,
    openTodoCount,
    todoLinks,
    todosFor,
    counts,
    totalLanes,
    actions,
  };
}
