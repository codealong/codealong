// The Sessions view: every session in the project, Pinned first in your order, each with a pin.
// The rest go by status under the headings the other views have (Needs you, Your turn, Working,
// Snoozed, then the days they were last active), or by when they started, under the days they
// started; the bar over the list picks which, and two lines a row or one (`SessionsBar`). Drag a
// pinned session, or press ⌃⌘↑ ⌃⌘↓, to reorder; drag any session up among them to pin it there,
// a pinned one down to unpin it, and any out of the window to pin it to the desktop
// (useRowDrag.tsx). ⌘-click and ⇧-click (or ⇧↑ ⇧↓) pick several, to mark seen, snooze, close,
// mark done or archive together (picks.ts, SessionPickPanel.tsx).

import { bySession, byStatusTime, startedOf } from '../lanes';
import type { SelectHow } from '../picks';
import type { Bay, DesktopPinned, Lane, Session, Status, Todo } from '../types';
import { AgentRow } from './LaneRow';
import { PinnedSection } from './PinnedSection';
import { BAY_WORD, BayHeader, SectionHeader, Seg } from './primitives';
import { useRowDrag, type DragProps } from './useRowDrag';

export interface SessionItem {
  kind: 'agent';
  key: string;
  lane: Lane;
  session: Session;
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

/** Snoozed and not running: it waits apart until its time, as on the floating bar. A running
 *  one stays with Working. */
function snoozedApart(s: Session): s is Session & { snoozedUntil: number } {
  return s.snoozedUntil != null && s.status !== 'working';
}

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
 */
export function sessionSections(rows: SessionItem[], sort: SessionSort, now: number): SessionSection[] {
  if (sort === 'started') {
    const newest = [...rows].sort((a, b) => startedOf(b.session) - startedOf(a.session) || a.key.localeCompare(b.key));
    return byDay(newest, (r) => startedOf(r.session), now).filter((s) => s.rows.length > 0);
  }
  const attention = ATTENTION.map(({ bay, statuses }) => ({ id: bay, label: BAY_WORD[bay], bay, rows: rows.filter(inGroup(statuses)).sort(byGroupTime) }));
  const until = (r: SessionItem) => r.session.snoozedUntil ?? 0;
  const snoozed = { id: 'snoozed', label: 'Snoozed', icon: 'zzz', rows: rows.filter((r) => snoozedApart(r.session)).sort((a, b) => until(a) - until(b) || a.key.localeCompare(b.key)) };
  const rest = rows.filter((r) => !snoozedApart(r.session) && !ATTENTION.some((g) => g.statuses.includes(r.session.status))).sort((a, b) => bySession(a.session, b.session));
  return [...attention, snoozed, ...byDay(rest, (r) => r.session.lastActivity, now)].filter((s) => s.rows.length > 0);
}

/** The sessions in `sort`'s order, as their headings list them: the order the keyboard goes in. */
export function sortSessions(rows: SessionItem[], sort: SessionSort, now: number): SessionItem[] {
  return sessionSections(rows, sort, now).flatMap((s) => s.rows);
}

/** The bar over the list, outside its listbox: how the list is sorted and how much each row
 *  says, as two groups of icon buttons, each named by its tooltip. It stays at the top while the
 *  list scrolls under it. */
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
  /** A click on the PR of a row's worktree: opens it on GitHub. */
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
  onShowTodo?: (todo: Todo) => void;
}

export function SessionsView(props: SessionsViewProps) {
  const { pinned, sections, sort, lines, now, selectedKey, picked, domId, onSelect, onOpen, onContextMenu, onPrMenu, onPrOpen, onPlacePinned, onUnpin, desktop, archived, showArchived, onToggleArchived, todosFor, onShowTodo } = props;
  const byKey = (key: string) => pinned.find((r) => r.session.key === key) ?? sections.flatMap((s) => s.rows).find((r) => r.session.key === key);
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
      onPrOpen={() => onPrOpen(r)}
      todos={todosFor?.(r.session.key)}
      onShowTodo={onShowTodo}
      drag={held}
    />
  );

  return (
    <>
      {pinned.length + sections.length > 0 && (
        <div className="ca-sessionlist" data-drag-list>
          {/* The first pin lands at the top of the list. */}
          {pinned.length === 0 && drag.dropIndex === 0 && <div className="ca-dropline" />}
          {pinned.length > 0 && <PinnedSection items={pinned} row={(r, i) => row(r, i, drag.rowProps(r.session.key, i, pinned.length))} />}
          {sections.map((s) => (
            <section key={s.id} className="ca-bayblock">
              {s.bay ? <BayHeader bay={s.bay} count={s.rows.length} /> : <SectionHeader icon={s.icon} label={s.label} count={s.rows.length} />}
              <div className="ca-bay-lanes">{s.rows.map((r) => row(r, null, drag.rowProps(r.session.key, null, pinned.length)))}</div>
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
          {showArchived && <div className="ca-bay-lanes">{archived.map((r) => row(r, null))}</div>}
        </section>
      )}
      {drag.ghost}
    </>
  );
}
