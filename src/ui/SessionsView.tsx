// The Sessions view: every session in the project in one list, pinned ones first in your order
// and marked with a pin. The rest go by status, as the floating bar orders them, or by when they
// started. Drag a pinned session, or press ⌃⌘↑ ⌃⌘↓, to reorder; drag any session up among them
// to pin it there, a pinned one down to unpin it, and any out of the window to pin it to the
// desktop (useRowDrag.tsx).

import { bySession, byStatusTime, startedOf } from '../lanes';
import type { MouseEvent } from 'react';
import type { DesktopPinned, Lane, Session, Status, Todo } from '../types';
import { AgentRow } from './LaneRow';
import { SectionHeader } from './primitives';
import { useRowDrag, type DragProps } from './useRowDrag';

export interface SessionItem {
  kind: 'agent';
  key: string;
  lane: Lane;
  session: Session;
}

/** How the sessions that aren't pinned are ordered. */
export type SessionSort = 'status' | 'started';

export const SESSION_SORTS: { id: SessionSort; label: string; title: string }[] = [
  { id: 'status', label: 'Status', title: 'What needs you first, then your turns, working agents and snoozed ones, then the rest by when they were last active' },
  { id: 'started', label: 'Started', title: 'The newest session first' },
];

/** The bar's groups, most urgent first: what needs you, turns that ended, then running agents. */
const ATTENTION: { statuses: Status[]; queue: boolean }[] = [
  { statuses: ['needs', 'error'], queue: true },
  { statuses: ['unread'], queue: true },
  { statuses: ['working'], queue: false },
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
 * pinned ones too, Needs you the longest wait first, then Your turn the newest first, as sorting
 * by status lists them. Snoozed ones wait their time.
 */
export function waitingQueue(rows: SessionItem[]): SessionItem[] {
  return ATTENTION.filter((g) => g.queue).flatMap(({ statuses }) => rows.filter(inGroup(statuses)).sort(byGroupTime));
}

/**
 * The sessions in `sort`'s order. By status: Needs you, Your turn and Working first, in the
 * floating bar's order (Needs you and Working the longest first, Your turn the newest status
 * first), then Snoozed, the soonest back first, then the rest by the local day they were last
 * active, each day the one you last sent a prompt first. By start: the newest first.
 */
export function sortSessions(rows: SessionItem[], sort: SessionSort, now: number): SessionItem[] {
  if (sort === 'started') return [...rows].sort((a, b) => startedOf(b.session) - startedOf(a.session) || a.key.localeCompare(b.key));
  const attention = ATTENTION.flatMap(({ statuses }) => rows.filter(inGroup(statuses)).sort(byGroupTime));
  const until = (r: SessionItem) => r.session.snoozedUntil ?? 0;
  const snoozed = rows.filter((r) => snoozedApart(r.session)).sort((a, b) => until(a) - until(b) || a.key.localeCompare(b.key));
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  const daysBack = (n: number) => {
    const d = new Date(today);
    d.setDate(d.getDate() - n);
    return d.getTime();
  };
  // Today, yesterday, the last 7 days, older.
  const [t0, t1, t6] = [today.getTime(), daysBack(1), daysBack(6)];
  const day = (r: SessionItem) => {
    const t = r.session.lastActivity;
    return t >= t0 ? 0 : t >= t1 ? 1 : t >= t6 ? 2 : 3;
  };
  const rest = rows
    .filter((r) => !snoozedApart(r.session) && !ATTENTION.some((g) => g.statuses.includes(r.session.status)))
    .sort((a, b) => day(a) - day(b) || bySession(a.session, b.session));
  return [...attention, ...snoozed, ...rest];
}

interface SessionsViewProps {
  pinned: SessionItem[];
  /** The rest, in `sort`'s order. */
  rows: SessionItem[];
  /** Sorted by start, the rows show when each started where they'd show when it was last active. */
  sort: SessionSort;
  now: number;
  selectedKey: string | null;
  domId: (row: SessionItem) => string;
  /** A row pressed, with the press: which button, which keys. */
  onSelect: (row: SessionItem, e: MouseEvent) => void;
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
  const { pinned, rows, sort, now, selectedKey, domId, onSelect, onOpen, onContextMenu, onPrMenu, onPrOpen, onPlacePinned, onUnpin, desktop, archived, showArchived, onToggleArchived, todosFor, onShowTodo } = props;
  const byKey = (key: string) => pinned.find((r) => r.session.key === key) ?? rows.find((r) => r.session.key === key);
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
  const row = (r: SessionItem, index: number | null, props?: DragProps) => (
    <AgentRow
      key={r.key}
      lane={r.lane}
      session={r.session}
      domId={domId(r)}
      now={now}
      age={sort === 'started' ? 'started' : 'active'}
      selected={r.key === selectedKey}
      onSelect={(e) => onSelect(r, e)}
      onOpen={() => onOpen(r)}
      pinned={index != null}
      onContextMenu={() => onContextMenu(r)}
      onPrMenu={() => onPrMenu(r)}
      onPrOpen={() => onPrOpen(r)}
      todos={todosFor?.(r.session.key)}
      onShowTodo={onShowTodo}
      {...props}
    />
  );

  return (
    <>
      {pinned.length + rows.length > 0 && (
        <section className="ca-bayblock ca-sessionlist">
          <div className="ca-bay-lanes" data-drag-list>
            {/* The first pin lands at the top of the list. */}
            {pinned.length === 0 && drag.dropIndex === 0 && <div className="ca-dropline" />}
            {pinned.map((r, i) => row(r, i, drag.rowProps(r.session.key, i, pinned.length)))}
            {rows.map((r) => row(r, null, drag.rowProps(r.session.key, null, pinned.length)))}
          </div>
        </section>
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
