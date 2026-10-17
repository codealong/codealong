// The Sessions view: every session in the project, Pinned first in your order.
// The rest go by status under the headings the other views have (Needs you, Your turn, Working,
// Snoozed, then the days they were last active), or by when they started, under the days they
// started; the bar over the list picks which, and two lines a row or one (`SessionsBar`). Drag a
// pinned session, or press ⌃⌘↑ ⌃⌘↓, to reorder; drag any session up among them to pin it there,
// a pinned one down to unpin it, and any out of the window to pin it to the desktop
// (useRowDrag.tsx). ⌘-click and ⇧-click (or ⇧↑ ⇧↓) pick several, to mark seen, snooze, close,
// mark done or archive together (picks.ts, SessionPickPanel.tsx).
//
// What works for a session goes under its row, set in: its helpers at work, its side conversation
// while one answers (LaneRow.tsx `SideRow`, which is no session: a click shows it above the
// session's box), and every one of a kind where its chip's double-click asked, with a row at the
// end that takes them away again (`FoldRow`).

import { Fragment } from 'react';
import { bySession, byStatusTime, foldsOf, runsShownKey, sidesListed, sidesShownKey, snoozedApart, startedOf, waitingSince, workingFor, type Fold } from '../lanes';
import type { SelectHow } from '../picks';
import type { HelperCount, RunCount, SideCount } from '../boardLists';
import type { SaidLine } from '../quick';
import type { Bay, DesktopPinned, Lane, Session, SideGlance, Status, Todo } from '../types';
import type { HelperActions } from './HelpersCard';
import { AgentRow, FoldRow, SideRow } from './LaneRow';
import { PinnedSection } from './PinnedSection';
import { BAY_WORD, BayHeader, SectionHeader, Seg } from './primitives';
import { useRowDrag, type DragProps } from './useRowDrag';

export interface SessionItem {
  kind: 'agent';
  key: string;
  lane: Lane;
  session: Session;
  /** A helper or a headless run listed under the session it works for (`sessionSections`). */
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

/** How much each session's row says: where it runs, its todo and its account on a second line,
 *  or one line with its worktree and PR at the end. */
export type SessionRows = 'two' | 'one';

export const SESSION_ROWS: { id: SessionRows; label: string; icon: string; title: string }[] = [
  { id: 'two', label: 'Two lines', icon: 'baseline-density-medium', title: 'Two lines: under each session, its worktree, todo, account, PR and diff' },
  { id: 'one', label: 'One line', icon: 'baseline-density-small', title: 'One line: each session with its worktree and PR at the end' },
];

/** A heading in the list and the sessions under it. The ones for what wants you or is running
 *  head as the Worktrees view's bays do; the rest as its other sections. */
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

/**
 * What a session's panel works through with ⌘⏎, `e` and ⌘]: the sessions listed that wait on you,
 * pinned ones too, Needs you then Your turn, each the longest wait first, as sorting by status
 * lists them and the queue goes through them (queue.ts `byPlace`). Snoozed ones wait their time.
 */
export function waitingQueue(rows: SessionItem[]): SessionItem[] {
  return ATTENTION.filter((g) => g.queue).flatMap(({ statuses }) => rows.filter(inGroup(statuses)).sort(byGroupTime));
}

const DAYS = [
  { id: 'today', label: 'Today' },
  { id: 'yesterday', label: 'Yesterday' },
  { id: 'week', label: 'Last 7 days' },
  { id: 'older', label: 'Older' },
];

/** `rows`, already in order, under Today, Yesterday, Last 7 days and Older, by `at` in local time. */
function byDay(rows: SessionItem[], at: (r: SessionItem) => number, now: number): SessionSection[] {
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  const daysBack = (n: number) => {
    const d = new Date(today);
    d.setDate(d.getDate() - n);
    return d.getTime();
  };
  const [t0, t1, t6] = [today.getTime(), daysBack(1), daysBack(6)];
  const day = (r: SessionItem) => {
    const t = at(r);
    return t >= t0 ? 0 : t >= t1 ? 1 : t >= t6 ? 2 : 3;
  };
  return DAYS.map((d, i) => ({ ...d, rows: rows.filter((r) => day(r) === i) }));
}

/**
 * The sessions under their headings, in `sort`'s order; a heading with none under it is left out.
 * By status: Needs you, Your turn and Working first, in the floating bar's order (each the longest
 * first), then Snoozed, the soonest back first, then the rest under the local day they were last
 * active, each day the one you last sent a prompt first. By start: under the day each started,
 * the newest first.
 *
 * A helper or headless run listed with the session it works for (lanes.ts `workingFor`) goes
 * under that session (`nested`), its own the most urgent first. The two go where the session
 * would, unless the helper waits on you and the session doesn't: then they're in Needs you, by
 * the helper's wait, so what it asks is seen beside the work it's for.
 */
export function sessionSections(rows: SessionItem[], sort: SessionSort, now: number): SessionSection[] {
  const { top, under } = families(rows);
  // What a session's place goes by: itself, or the helper under it that has waited on you longest.
  const lead = (r: SessionItem): Session => {
    if (waitingSince(r.session) != null) return r.session;
    const waiting = (under.get(r.session.key) ?? []).filter((k) => waitingSince(k.session) != null && k.session.snoozedUntil == null).sort(byGroupTime);
    return waiting[0]?.session ?? r.session;
  };
  const withHelpers = (s: SessionSection): SessionSection => ({ ...s, rows: s.rows.flatMap((r) => [r, ...(under.get(r.session.key) ?? [])]) });
  if (sort === 'started') {
    const newest = [...top].sort((a, b) => startedOf(b.session) - startedOf(a.session) || a.key.localeCompare(b.key));
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
 *  it's listed once; a pinned helper's own helpers go under it. */
export function helpersUnder(rows: SessionItem[], heads: ReadonlySet<string>): Map<string, SessionItem[]> {
  const under = new Map<string, SessionItem[]>();
  if (heads.size === 0) return under;
  const byKey = new Map(rows.map((r) => [r.session.key, r]));
  for (const r of rows) {
    if (heads.has(r.session.key)) continue;
    let at = r;
    for (let i = 0; i < 4; i++) {
      const up = workingFor(at.session);
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

/** The rows that lead, and under each session's key the helpers listed with it, the most urgent
 *  first. Only one deep: a helper's own helpers go under the session at the top. */
function families(rows: SessionItem[]): { top: SessionItem[]; under: Map<string, SessionItem[]> } {
  const under = new Map<string, SessionItem[]>();
  if (!rows.some((r) => workingFor(r.session))) return { top: rows, under };
  const byKey = new Map(rows.map((r) => [r.session.key, r]));
  // The listed session a row goes under, climbing past helpers to the one at the top.
  const headOf = (r: SessionItem): SessionItem | null => {
    let at = r;
    for (let i = 0; i < 4; i++) {
      const up = workingFor(at.session);
      const parent = up ? byKey.get(up) : undefined;
      if (!parent || parent === r) return at === r ? null : at;
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
export function sortSessions(rows: SessionItem[], sort: SessionSort, now: number): SessionItem[] {
  return sessionSections(rows, sort, now).flatMap((s) => s.rows);
}

/** Sessions' tools in the bar over the list (ListBar.tsx), outside its listbox: how the list is
 *  sorted and how much each row says, as two groups of icon buttons, each named by its tooltip. */
export function SessionsBar({ sort, onSort, lines, onLines }: { sort: SessionSort; onSort: (sort: SessionSort) => void; lines: SessionRows; onLines: (lines: SessionRows) => void }) {
  return (
    <div className="ca-sessionbar" role="toolbar" aria-label="How Sessions is listed">
      <Seg className="ca-sessionbar-seg" label="Sort by" look="icon" iconSize={16} options={SESSION_SORTS} value={sort} onPick={onSort} />
      <Seg className="ca-sessionbar-seg" label="Rows" look="icon" iconSize={16} options={SESSION_ROWS} value={lines} onPick={onLines} />
    </div>
  );
}

interface SessionsViewProps {
  pinned: SessionItem[];
  /** The helpers listed under each pinned session (boardLists.ts `helpersUnder`). */
  pinnedHelpers?: Map<string, SessionItem[]>;
  /** The rest under their headings, in `sort`'s order. */
  sections: SessionSection[];
  /** Sorted by start, the rows show when each started where they'd show when it was last active. */
  sort: SessionSort;
  /** Two lines a session, or one. */
  lines: SessionRows;
  now: number;
  selectedKey: string | null;
  /** The rows picked together with the selected one, when there are several. */
  picked: ReadonlySet<string>;
  domId: (row: SessionItem) => string;
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
  const { pinned, pinnedHelpers, sections, sort, lines, now, selectedKey, picked, domId, onSelect, onOpen, onContextMenu, onPrMenu, onPrShow, onPrOpen, onPlacePinned, onUnpin, desktop, archived, showArchived, onToggleArchived, todosFor, helpersOf, helpersShown, onToggleHelpers, runsOf, onToggleRuns, sidesOf, sideGlancesOf, onToggleSides, onShowSide, helperActions, onShowTodo, saidOf } = props;
  const shown = helpersShown ?? NO_KEYS;
  const byKey = (key: string) => pinned.find((r) => r.session.key === key) ?? [...(pinnedHelpers?.values() ?? [])].flat().find((r) => r.session.key === key) ?? sections.flatMap((s) => s.rows).find((r) => r.session.key === key);
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

  /** A session's row: `index` is its place among the pinned ones; the archived aren't dragged. */
  const row = (r: SessionItem, index: number | null, held?: DragProps) => (
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
  const family = (head: SessionItem, kids: SessionItem[], index: number | null, held: (r: SessionItem, index: number | null) => DragProps | undefined) => {
    const folds = foldsOf([head, ...kids].map((r) => r.session), shown, { helpers: (k) => helpersOf?.(k), runs: (k) => runsOf?.(k), sides: (k) => sidesOf?.(k) });
    return (
      <Fragment key={head.key}>
        {row(head, index, held(head, index))}
        {sideRows(head)}
        {kids.map((k) => (
          <Fragment key={k.key}>
            {row(k, null, held(k, null))}
            {sideRows(k)}
          </Fragment>
        ))}
        {folds.map((f) => (
          <FoldRow key={`fold:${f.kind}:${f.key}`} fold={f} head={f.key === head.session.key} onFold={fold} />
        ))}
      </Fragment>
    );
  };

  return (
    <>
      {pinned.length + sections.length > 0 && (
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
          {sections.map((s) => (
            <section key={s.id} className="ca-bayblock">
              {s.bay ? <BayHeader bay={s.bay} count={s.rows.length} /> : <SectionHeader icon={s.icon} label={s.label} count={s.rows.length} />}
              <div className="ca-bay-lanes">{familiesOf(s.rows).map(({ head, kids }) => family(head, kids, null, (x) => drag.rowProps(x.session.key, null, pinned.length)))}</div>
            </section>
          ))}
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
    </>
  );
}
