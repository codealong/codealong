// The Sessions view: every session in the project, Pinned first in your order.
// The rest go by status under the headings the other views have (Needs you, Your turn, Working,
// Snoozed, then the days they were last active), or by when they started, under the days they
// started; the bar over the list picks which, whether they're grouped by worktree or by repo,
// whether a session another one started goes under that one, and two lines a row or one
// (`SessionsBar`). Drag a pinned session, or press ⌃⌘↑ ⌃⌘↓, to reorder;
// drag any session up among them to pin it there, a pinned one down to unpin it, and any out of
// the window to pin it to the desktop (useRowDrag.tsx). ⌘-click and ⇧-click (or ⇧↑ ⇧↓) pick
// several, to mark seen, snooze, close, mark done or archive together (picks.ts,
// SessionPickPanel.tsx).
//
// Grouped (`groupSessions`), each worktree or repo heads its sessions once, under the heading its
// first session is under: a worktree whose session needs you is under Needs you with the rest of
// its sessions. A worktree's heading says where it stands (LaneRow.tsx `WorktreeHead`) and is a
// row of its own, selected for its panel, picked with others and pinned as a session is; pinned
// groups follow the pinned sessions, in your order. A group lists its first few sessions and folds
// the rest away until it's opened (`GROUP_SHOWN`).
//
// What works for a session goes under its row, set in: its helpers at work, its side conversation
// while one answers (LaneRow.tsx `SideRow`, which is no session: a click shows it above the
// session's box), and every one of a kind where its chip's double-click asked, with a row at the
// end that takes them away again (`FoldRow`); and, with Started sessions under their session, the
// sessions its agent started.

import { Fragment } from 'react';
import { byDay, newestFirst } from '../days';
import { groupOptions, groupRows, groupsOf, placeIn, type GroupBy, type Grouped, type GroupOptions, type ListGroup, type ListGroupBy } from '../groups';
import { bySession, byStatusTime, foldsOf, listedUnder, runsShownKey, sidesListed, sidesShownKey, snoozedApart, startedOf, waitingSince, workingFor, type Fold, type LaneItem } from '../lanes';
import type { SelectHow } from '../picks';
import type { HelperCount, RunCount, SideCount } from '../boardLists';
import type { SaidLine } from '../quick';
import type { Bay, DesktopPinned, Lane, Session, SideGlance, Status, Todo } from '../types';
import type { HelperActions } from './HelpersCard';
import { GroupBlock, PinnedGroups, useGroupDrag, type GroupActs } from './Groups';
import { AgentRow, FoldRow, SideRow } from './LaneRow';
import { PinnedSection } from './PinnedSection';
import { ListSeg, ListTools, RowsSeg, rowsOptions, type RowLines } from './ListBar';
import { BAY_WORD, BayHeader, SectionHeader } from './primitives';
import { useRowDrag, type DragProps } from './useRowDrag';

export interface SessionItem {
  kind: 'agent';
  key: string;
  lane: Lane;
  session: Session;
  /** A helper, a headless run or a session another one started, listed under that session
   *  (`sessionSections`). */
  nested?: boolean;
  /** A helper listed under a session in another worktree than its own, which its row then names. */
  elsewhere?: boolean;
}

/** How the sessions that aren't pinned are ordered. */
export type SessionSort = 'status' | 'started';

export const SESSION_SORTS: { id: SessionSort; label: string; icon: string; title: string }[] = [
  { id: 'status', label: 'Status', icon: 'sort-descending-shapes', title: 'Sort by status: what needs you first, then your turns, working agents and snoozed ones, then the rest by the day they were last active' },
  { id: 'started', label: 'Started', icon: 'sort-descending-numbers', title: 'Sort by start: the newest session first, under the day it started' },
];

/** Where a session another session's agent started is listed: on its own, or under that session
 *  as its helpers are (lanes.ts `listedUnder`). */
export type StartedPlace = 'apart' | 'under';

export const STARTED_PLACES: { id: StartedPlace; label: string; icon: string; title: string }[] = [
  { id: 'apart', label: 'On their own', icon: 'layout-rows', title: 'Started sessions on their own: a session another session’s agent started is listed as any other' },
  { id: 'under', label: 'Under their session', icon: 'subtask', title: 'Started sessions under their session: a session another session’s agent started (codealong start, a task the Claude app offered) is listed under that session, as its helpers are' },
];

/** How much each session's row says: where it runs, its todo and its account on a second line,
 *  or one line with its worktree and PR at the end. */
export const SESSION_ROWS = rowsOptions('under each session, its worktree, todo, account, PR and diff', 'each session with its worktree and PR at the end');

/** Whether the sessions that aren't pinned are grouped, and by what (groups.ts). */
export type SessionGroupBy = ListGroupBy;

export const SESSION_GROUPS = groupOptions('sessions');

/** A worktree or a repo heading its sessions in Sessions grouped by it. */
export type SessionGroup = ListGroup<SessionItem>;
export type GroupedSessions = Grouped<SessionSection, SessionItem>;

/** What a group's key is for a worktree: its own id, or its repo's. */
export function groupKeyOf(lane: Lane, by: GroupBy): string {
  return placeIn(lane, by).key;
}

/**
 * The sections `sessionSections` made, grouped by worktree or by repo (groups.ts `groupRows`): a
 * session with the rows nested under it is listed and folded whole, in its group whatever
 * worktree they're in, and one that waits on you or is working never folds away, nor one with
 * such a helper under it, whatever the sort: by start, one can come after five idle ones.
 */
export function groupSessions(sections: SessionSection[], by: GroupBy, o: Omit<GroupOptions<SessionItem>, 'units' | 'awake' | 'apart'>): GroupedSessions {
  return groupRows(sections, by, (r) => placeIn(r.lane, by), {
    ...o,
    units: (rows) => familiesOf(rows).map(({ head, kids }) => [head, ...kids]),
    awake: (unit) => unit.some((r) => ATTENTION.some((g) => inGroup(g.statuses)(r))),
  });
}

/** A worktree's heading as a row of the list: selected for its panel, and picked with others. */
export function headItem(lane: Lane): LaneItem {
  return { kind: 'lane', key: lane.id, lane };
}

/** The grouped list's rows top to bottom, as the keyboard goes through them: each worktree's
 *  heading, then its sessions. A repo's heading isn't a row. */
export function groupedRows(g: GroupedSessions): (SessionItem | LaneItem)[] {
  return groupsOf(g).flatMap((x): (SessionItem | LaneItem)[] => (x.lane ? [headItem(x.lane), ...x.rows] : x.rows));
}

/** A heading in the list and the sessions under it. The ones for what wants you or is running
 *  head as bays do; the rest as sections. */
export interface SessionSection {
  id: string;
  label: string;
  bay?: Bay;
  icon?: string;
  rows: SessionItem[];
}

/** The bar's groups, most urgent first: what needs you, turns that ended, then running agents. */
const ATTENTION: { bay: Bay; statuses: Status[]; queue: boolean }[] = [
  { bay: 'needs', statuses: ['needs', 'error'], queue: true },
  { bay: 'unread', statuses: ['unread'], queue: true },
  { bay: 'working', statuses: ['working'], queue: false },
];

const inGroup = (statuses: Status[]) => (r: SessionItem) => statuses.includes(r.session.status) && !snoozedApart(r.session);
const byGroupTime = (a: SessionItem, b: SessionItem) => byStatusTime(a.session, b.session) || a.key.localeCompare(b.key);
/** Which of the bar's groups a session is in, the most urgent 0; past them all when it's in none. */
const urgency = (s: Session) => {
  const i = ATTENTION.findIndex(({ statuses }) => statuses.includes(s.status) && !snoozedApart(s));
  return i < 0 ? ATTENTION.length : i;
};

/**
 * What a session's panel works through with ⌘⏎, `e` and ⌘]: the sessions listed that wait on you,
 * pinned ones too, Needs you then Your turn, each the longest wait first, as sorting by status
 * lists them and the queue goes through them (queue.ts `byPlace`). Snoozed ones wait their time.
 */
export function waitingQueue(rows: SessionItem[]): SessionItem[] {
  return ATTENTION.filter((g) => g.queue).flatMap(({ statuses }) => rows.filter(inGroup(statuses)).sort(byGroupTime));
}

/**
 * The sessions under their headings, in `sort`'s order; a heading with none under it is left out.
 * By status: Needs you, Your turn and Working first, in the floating bar's order (each the longest
 * first), then Snoozed, the soonest back first, then the rest under the local day they were last
 * active, each day the one you last sent a prompt first. By start: under the day each started,
 * the newest first.
 *
 * A helper or headless run listed with the session it works for (lanes.ts `workingFor`) goes
 * under that session (`nested`), its own the most urgent first, and with `place` under, so does a
 * session it started (`listedUnder`). The two go where the session would, unless the helper waits
 * on you and the session doesn't: then they're in Needs you, by the helper's wait, so what it asks
 * is seen beside the work it's for. A session it started takes them as far up as it is itself
 * (Your turn, Working), as it would be on its own.
 */
export function sessionSections(rows: SessionItem[], sort: SessionSort, now: number, place: StartedPlace = 'apart'): SessionSection[] {
  const { top, under } = families(rows, place === 'under');
  // What a session's place goes by: itself, the helper under it that has waited on you longest, or
  // the session it started that's furthest up the bar's groups when that's further than itself.
  const lead = (r: SessionItem): Session => {
    if (waitingSince(r.session) != null) return r.session;
    const kids = under.get(r.session.key) ?? [];
    const waiting = kids.filter((k) => waitingSince(k.session) != null && k.session.snoozedUntil == null).sort(byGroupTime);
    if (waiting.length > 0) return waiting[0].session;
    const ahead = kids.filter((k) => !workingFor(k.session) && urgency(k.session) < urgency(r.session)).sort((a, b) => urgency(a.session) - urgency(b.session) || byGroupTime(a, b));
    return ahead[0]?.session ?? r.session;
  };
  const withHelpers = (s: SessionSection): SessionSection => ({ ...s, rows: s.rows.flatMap((r) => [r, ...(under.get(r.session.key) ?? [])]) });
  if (sort === 'started') {
    const newest = newestFirst(top, (r) => startedOf(r.session), (r) => r.key);
    return byDay(newest, (r) => startedOf(r.session), now)
      .filter((s) => s.rows.length > 0)
      .map(withHelpers);
  }
  const ranked = top.map((r) => ({ ...r, session: lead(r), row: r }));
  const rowsOf = (rs: typeof ranked) => rs.map((r) => r.row);
  const attention = ATTENTION.map(({ bay, statuses }) => ({ id: bay, label: BAY_WORD[bay], bay, rows: rowsOf(ranked.filter(inGroup(statuses)).sort(byGroupTime)) }));
  const until = (r: SessionItem) => r.session.snoozedUntil ?? 0;
  const snoozed = { id: 'snoozed', label: 'Snoozed', icon: 'zzz', rows: rowsOf(ranked.filter((r) => snoozedApart(r.session)).sort((a, b) => until(a) - until(b) || a.key.localeCompare(b.key))) };
  const rest = rowsOf(ranked.filter((r) => !snoozedApart(r.session) && !ATTENTION.some((g) => g.statuses.includes(r.session.status))).sort((a, b) => bySession(a.session, b.session)));
  return [...attention, snoozed, ...byDay(rest, (r) => r.session.lastActivity, now)].filter((s) => s.rows.length > 0).map(withHelpers);
}

/** The helpers listed under each of `heads` (pinned sessions, which Pinned lists apart), as
 *  `sessionSections` would list them under those sessions, the most urgent first: each under the
 *  nearest of its sessions that's one of them. One that's a head itself (pinned too) isn't, so
 *  it's listed once; a pinned helper's own helpers go under it. With `place` under, the sessions
 *  each started go under it too. */
export function helpersUnder(rows: SessionItem[], heads: ReadonlySet<string>, place: StartedPlace = 'apart'): Map<string, SessionItem[]> {
  const under = new Map<string, SessionItem[]>();
  if (heads.size === 0) return under;
  const byKey = new Map(rows.map((r) => [r.session.key, r]));
  for (const r of rows) {
    if (heads.has(r.session.key)) continue;
    let at = r;
    for (let i = 0; i < 4; i++) {
      const up = listedUnder(at.session, place === 'under');
      const parent = up ? byKey.get(up) : undefined;
      if (!parent || parent === r) break;
      if (heads.has(parent.session.key)) {
        under.set(parent.session.key, [...(under.get(parent.session.key) ?? []), { ...r, nested: true, elsewhere: r.lane.id !== parent.lane.id }]);
        break;
      }
      at = parent;
    }
  }
  for (const kids of under.values()) kids.sort((a, b) => bySession(a.session, b.session));
  return under;
}

/** The rows that lead, and under each session's key the helpers listed with it (and with
 *  `started`, the sessions it started), the most urgent first. Only one deep: a helper's own
 *  helpers go under the session at the top. */
function families(rows: SessionItem[], started: boolean): { top: SessionItem[]; under: Map<string, SessionItem[]> } {
  const under = new Map<string, SessionItem[]>();
  if (!rows.some((r) => listedUnder(r.session, started))) return { top: rows, under };
  const byKey = new Map(rows.map((r) => [r.session.key, r]));
  // The listed session a row goes under, climbing past helpers to the one at the top. One whose
  // climb comes back to it leads, so sessions that name each other as their origin both show.
  const headOf = (r: SessionItem): SessionItem | null => {
    let at = r;
    for (let i = 0; i < 4; i++) {
      const up = listedUnder(at.session, started);
      const parent = up ? byKey.get(up) : undefined;
      if (parent === r) return null;
      if (!parent) return at === r ? null : at;
      at = parent;
    }
    return at === r ? null : at;
  };
  const top: SessionItem[] = [];
  for (const r of rows) {
    const head = headOf(r);
    if (!head) top.push(r);
    else under.set(head.session.key, [...(under.get(head.session.key) ?? []), { ...r, nested: true, elsewhere: r.lane.id !== head.lane.id }]);
  }
  for (const kids of under.values()) kids.sort((a, b) => bySession(a.session, b.session));
  return { top, under };
}

/** A section's rows a family at a time: each session at the top, with the rows nested under it
 *  (`sessionSections`), which follow it. */
export function familiesOf(rows: readonly SessionItem[]): { head: SessionItem; kids: SessionItem[] }[] {
  const out: { head: SessionItem; kids: SessionItem[] }[] = [];
  for (const r of rows) {
    const last = out[out.length - 1];
    if (r.nested && last) last.kids.push(r);
    else out.push({ head: r, kids: [] });
  }
  return out;
}

/** The sessions in `sort`'s order, as their headings list them: the order the keyboard goes in. */
export function sortSessions(rows: SessionItem[], sort: SessionSort, now: number, place: StartedPlace = 'apart'): SessionItem[] {
  return sessionSections(rows, sort, now, place).flatMap((s) => s.rows);
}

/** Sessions' tools in the bar over the list (ListBar.tsx): how the list is sorted, what groups
 *  it, where a session another one started goes, and how much each row says. */
export function SessionsBar({ sort, onSort, group, onGroup, started, onStarted, lines, onLines }: { sort: SessionSort; onSort: (sort: SessionSort) => void; group: SessionGroupBy; onGroup: (group: SessionGroupBy) => void; started: StartedPlace; onStarted: (place: StartedPlace) => void; lines: RowLines; onLines: (lines: RowLines) => void }) {
  return (
    <ListTools label="How Sessions is listed">
      <ListSeg label="Sort by" options={SESSION_SORTS} value={sort} onPick={onSort} />
      <ListSeg label="Group by" options={SESSION_GROUPS} value={group} onPick={onGroup} />
      <ListSeg label="Started sessions" options={STARTED_PLACES} value={started} onPick={onStarted} />
      <RowsSeg options={SESSION_ROWS} value={lines} onPick={onLines} />
    </ListTools>
  );
}

interface SessionsViewProps {
  pinned: SessionItem[];
  /** The helpers listed under each pinned session (boardLists.ts `helpersUnder`). */
  pinnedHelpers?: Map<string, SessionItem[]>;
  /** The rest under their headings, in `sort`'s order. */
  sections: SessionSection[];
  /** The rest grouped instead, when the list is grouped (`groupSessions`), and what the groups'
   *  headings do. */
  groups?: GroupedSessions | null;
  groupActs?: GroupActs;
  /** Sorted by start, the rows show when each started where they'd show when it was last active. */
  sort: SessionSort;
  /** Two lines a session, or one. */
  lines: RowLines;
  now: number;
  selectedKey: string | null;
  /** The rows picked together with the selected one, when there are several. */
  picked: ReadonlySet<string>;
  /** A row's element id, a session's or a worktree's heading's, by its key. */
  domId: (row: { key: string }) => string;
  /** A row pressed with the primary button, alone or with ⌘ or ⇧: a right-click leaves the
   *  selection alone. */
  onSelect: (row: SessionItem, how: SelectHow) => void;
  onOpen: (row: SessionItem) => void;
  /** Right-click on a row. */
  onContextMenu: (row: SessionItem) => void;
  /** Right-click on the PR of a row's worktree. */
  onPrMenu: (row: SessionItem) => void;
  /** A click on the PR of a row's worktree: shows it in PRs. */
  onPrShow: (row: SessionItem) => void;
  /** A double-click on it: opens it on GitHub. */
  onPrOpen: (row: SessionItem) => void;
  /** Puts a session at insertion point `index` among the pinned ones showing: a pinned one
   *  moves there, another is pinned there. */
  onPlacePinned: (key: string, index: number) => void;
  /** Unpins a session: its row dragged from the pinned ones down to the list, or out of the window. */
  onUnpin: (key: string) => void;
  /** What a session's row pins to the desktop when it's dragged out of the window, if it can be. */
  desktop: (row: SessionItem) => DesktopPinned | null;
  /** Archived sessions, listed under a fold at the end. */
  archived: SessionItem[];
  showArchived: boolean;
  onToggleArchived: () => void;
  /** Todos a session is linked to, for its chip. */
  todosFor?: (sessionKey: string) => Todo[] | undefined;
  /** A session's helpers, for their chip on its row. */
  helpersOf?: (sessionKey: string) => HelperCount | undefined;
  /** The sessions whose every helper the list shows under them, which their chip's double-click
   *  turns on and off (`onToggleHelpers`). */
  helpersShown?: ReadonlySet<string>;
  onToggleHelpers?: (sessionKey: string) => void;
  /** A session's headless runs, for their chip on its row; all of them are listed under it while
   *  `helpersShown` holds its `runsShownKey`, which their chip's double-click turns on and off
   *  (`onToggleRuns`). */
  runsOf?: (sessionKey: string) => RunCount | undefined;
  onToggleRuns?: (sessionKey: string) => void;
  /** A session's side conversations, counted on its row's chip and listed under it while one
   *  answers, or all of them while `helpersShown` holds its `sidesShownKey`, which their chip's
   *  double-click turns on and off (`onToggleSides`). */
  sidesOf?: (sessionKey: string) => SideCount | undefined;
  sideGlancesOf?: (sessionKey: string) => SideGlance[] | undefined;
  onToggleSides?: (sessionKey: string) => void;
  /** Shows a side conversation: its session selected, and it above the session's box. */
  onShowSide?: (sessionKey: string, side: SideGlance) => void;
  /** What a helpers chip's card does with a helper (HelpersCard.tsx). */
  helperActions?: HelperActions;
  onShowTodo?: (todo: Todo) => void;
  /** The line a session the search found by what was said in it shows (boardLists.ts
   *  `saidLinesOf`). */
  saidOf?: (sessionKey: string) => SaidLine | undefined;
}

const NO_KEYS: ReadonlySet<string> = new Set();

export function SessionsView(props: SessionsViewProps) {
  const { pinned, pinnedHelpers, sections, groups, groupActs, sort, lines, now, selectedKey, picked, domId, onSelect, onOpen, onContextMenu, onPrMenu, onPrShow, onPrOpen, onPlacePinned, onUnpin, desktop, archived, showArchived, onToggleArchived, todosFor, helpersOf, helpersShown, onToggleHelpers, runsOf, onToggleRuns, sidesOf, sideGlancesOf, onToggleSides, onShowSide, helperActions, onShowTodo, saidOf } = props;
  const shown = helpersShown ?? NO_KEYS;
  const allGroups = groups ? groupsOf(groups) : [];
  const listed = groups ? allGroups.flatMap((g) => g.rows) : sections.flatMap((s) => s.rows);
  const byKey = (key: string) => pinned.find((r) => r.session.key === key) ?? [...(pinnedHelpers?.values() ?? [])].flat().find((r) => r.session.key === key) ?? listed.find((r) => r.session.key === key);
  // Only the pinned rows carry an index: the others join them when dropped among them.
  const drag = useRowDrag({
    onPlace: onPlacePinned,
    joins: true,
    onLeave: onUnpin,
    desktop: (key) => {
      const r = byKey(key);
      return r ? desktop(r) : null;
    },
    label: (key) => byKey(key)?.session.title ?? '',
  });
  // The groups' headings are dragged apart from the sessions: among the pinned groups, or out of
  // the window.
  const groupDrag = useGroupDrag(allGroups, groupActs);

  /** A session's row: `index` is its place among the pinned ones; the archived aren't dragged.
   *  In a worktree's group it leaves out what the heading says: the worktree, its PR and diff. */
  const row = (r: SessionItem, index: number | null, held?: DragProps, inWorktree = false) => (
    <AgentRow
      key={r.key}
      lane={r.lane}
      session={r.session}
      domId={domId(r)}
      now={now}
      age={sort === 'started' ? 'started' : 'active'}
      slim={lines === 'one'}
      selected={r.key === selectedKey || picked.has(r.key)}
      onSelect={(how) => onSelect(r, how)}
      onOpen={() => onOpen(r)}
      pinned={index != null}
      onContextMenu={() => onContextMenu(r)}
      onPrMenu={() => onPrMenu(r)}
      onPrShow={() => onPrShow(r)}
      onPrOpen={() => onPrOpen(r)}
      todos={todosFor?.(r.session.key)}
      helpers={helpersOf?.(r.session.key)}
      helpersShown={!!helpersShown?.has(r.session.key)}
      onToggleHelpers={onToggleHelpers && (() => onToggleHelpers(r.session.key))}
      runs={runsOf?.(r.session.key)}
      runsShown={!!helpersShown?.has(runsShownKey(r.session.key))}
      onToggleRuns={onToggleRuns && (() => onToggleRuns(r.session.key))}
      sides={sidesOf?.(r.session.key)}
      sidesShown={!!helpersShown?.has(sidesShownKey(r.session.key))}
      onToggleSides={onToggleSides && (() => onToggleSides(r.session.key))}
      sideActions={onShowSide && sideActionsOf(r.session.key)}
      helperActions={helperActions}
      nested={r.nested}
      elsewhere={r.elsewhere}
      inWorktree={inWorktree}
      onShowTodo={onShowTodo}
      said={saidOf?.(r.session.key)}
      drag={held}
    />
  );

  /** What a side conversations chip's card does with one, by its id: shows it. */
  const sideActionsOf = (key: string): HelperActions => ({
    show: (id) => {
      const g = sideGlancesOf?.(key)?.find((x) => x.id === id);
      if (g) onShowSide?.(key, g);
    },
  });
  /** A session's side conversations listed under it (lanes.ts `sidesListed`). */
  const sideRows = (r: SessionItem) =>
    sidesListed(sideGlancesOf?.(r.session.key), r.session.key, shown).map((g) => <SideRow key={`side:${g.id}`} sessionKey={r.session.key} side={g} now={now} onShow={(key, side) => onShowSide?.(key, side)} />);
  /** Back to listing only the ones at work, as the kind's chip's double-click does. */
  const fold = (f: Fold) => (f.kind === 'helpers' ? onToggleHelpers : f.kind === 'runs' ? onToggleRuns : onToggleSides)?.(f.key);
  /** A session's row, its side conversations, the rows nested under it each with theirs, and the
   *  way back from what's listed in full under any of them. `held` is how each row is dragged. */
  const family = (head: SessionItem, kids: SessionItem[], index: number | null, held: (r: SessionItem, index: number | null) => DragProps | undefined, inWorktree = false) => {
    const folds = foldsOf([head, ...kids].map((r) => r.session), shown, { helpers: (k) => helpersOf?.(k), runs: (k) => runsOf?.(k), sides: (k) => sidesOf?.(k) });
    return (
      <Fragment key={head.key}>
        {row(head, index, held(head, index), inWorktree)}
        {sideRows(head)}
        {kids.map((k) => (
          <Fragment key={k.key}>
            {row(k, null, held(k, null), inWorktree)}
            {sideRows(k)}
          </Fragment>
        ))}
        {folds.map((f) => (
          <FoldRow key={`fold:${f.kind}:${f.key}`} fold={f} head={f.key === head.session.key} onFold={fold} />
        ))}
      </Fragment>
    );
  };
  const listedRow = (x: SessionItem) => drag.rowProps(x.session.key, null, pinned.length);

  /** A group: its heading, a row of its own for a worktree, its sessions, and the way to the ones
   *  folded away. `index` is its place among the pinned groups. */
  const group = (g: SessionGroup, index: number | null, acts: GroupActs, count: number) => (
    <GroupBlock key={`group:${g.key}`} g={g} acts={acts} now={now} rows domId={g.lane ? domId({ key: g.lane.id }) : undefined} selected={!!g.lane && selectedKey === g.lane.id} pinned={index != null} drag={groupDrag.rowProps(g.key, index, count)} none="">
      {familiesOf(g.rows).map(({ head, kids }) => family(head, kids, null, listedRow, !!g.lane))}
    </GroupBlock>
  );
  const listedCount = pinned.length + (groups ? groups.pinned.length + groups.sections.length : sections.length);
  return (
    <>
      {listedCount > 0 && (
        <div className="ca-sessionlist" data-drag-list>
          {/* The first pin lands at the top of the list. */}
          {pinned.length === 0 && drag.dropIndex === 0 && <div className="ca-dropline" />}
          {pinned.length > 0 && (
            <PinnedSection
              items={pinned}
              // Its helpers under it, not pinned themselves: they move with it.
              row={(r, i) => family(r, pinnedHelpers?.get(r.session.key) ?? [], i, (x, at) => drag.rowProps(x.session.key, at, pinned.length))}
            />
          )}
          {groups && groupActs ? (
            <>
              {/* The first pinned group lands after the pinned sessions. */}
              {groups.pinned.length === 0 && groupDrag.dropIndex === 0 && <div className="ca-dropline" />}
              {groups.pinned.length > 0 && (
                <PinnedGroups by={groups.by} count={groups.pinned.length} reorder>
                  {groups.pinned.map((g, i) => group(g, i, groupActs, groups.pinned.length))}
                </PinnedGroups>
              )}
              {groups.sections.map((s) => (
                <section key={s.id} className="ca-bayblock">
                  {s.bay ? <BayHeader bay={s.bay} count={s.count} /> : <SectionHeader icon={s.icon} label={s.label} count={s.count} />}
                  <div className="ca-bay-lanes">{s.groups.map((g) => group(g, null, groupActs, groups.pinned.length))}</div>
                </section>
              ))}
            </>
          ) : (
            sections.map((s) => (
              <section key={s.id} className="ca-bayblock">
                {s.bay ? <BayHeader bay={s.bay} count={s.rows.length} /> : <SectionHeader icon={s.icon} label={s.label} count={s.rows.length} />}
                <div className="ca-bay-lanes">{familiesOf(s.rows).map(({ head, kids }) => family(head, kids, null, listedRow))}</div>
              </section>
            ))
          )}
        </div>
      )}
      {archived.length > 0 && (
        <section className="ca-bayblock ca-archived">
          <SectionHeader
            icon="archive"
            label="Archived"
            count={archived.length}
            hint={
              <button type="button" className="ca-linkbtn" aria-expanded={showArchived} onClick={onToggleArchived}>
                {showArchived ? 'Hide' : 'Show'}
              </button>
            }
          />
          {showArchived && <div className="ca-bay-lanes">{archived.map((r) => family(r, [], null, () => undefined))}</div>}
        </section>
      )}
      {drag.ghost}
      {groupDrag.ghost}
    </>
  );
}
