// The Sessions view: every session in the project, pinned ones first in your order, then the
// ones that want you or are running, grouped as the floating bar groups them, then the rest by
// the day they were last active. Drag a pinned session, or press ⌥↑ ⌥↓, to reorder.

import { bySession } from '../lanes';
import type { Bay, Lane, Session, Status, Todo } from '../types';
import { AgentRow } from './LaneRow';
import { PinnedSection, type PinDrag } from './PinnedSection';
import { BayHeader, SectionHeader } from './primitives';

export interface SessionItem {
  kind: 'agent';
  key: string;
  lane: Lane;
  session: Session;
}

export interface SessionSection {
  id: string;
  label: string;
  /** Set on the sections for what wants you or is running, which head as the board's bays do. */
  bay?: Bay;
  rows: SessionItem[];
}

/** The bar's groups, most urgent first: what needs you, turns that ended, then running agents. */
const ATTENTION: { bay: Bay; label: string; statuses: Status[] }[] = [
  { bay: 'needs', label: 'Needs you', statuses: ['needs', 'error'] },
  { bay: 'unread', label: 'Your turn', statuses: ['unread'] },
  { bay: 'working', label: 'Working', statuses: ['working'] },
];

/**
 * Needs you, Your turn and Working first, as on the floating bar, each newest status first: a
 * status's time doesn't move while it lasts, so rows hold their places. The rest go under Today,
 * Yesterday, Last 7 days and Older, by last activity in local time, the one you last sent a
 * prompt first.
 */
export function timeSections(rows: SessionItem[], now: number): SessionSection[] {
  const since = (s: Session) => s.statusSince ?? s.lastActivity;
  const attention: SessionSection[] = ATTENTION.map(({ bay, label, statuses }) => ({
    id: bay,
    label,
    bay,
    rows: rows.filter((r) => statuses.includes(r.session.status)).sort((a, b) => since(b.session) - since(a.session) || a.key.localeCompare(b.key)),
  }));
  const rest = rows.filter((r) => !ATTENTION.some((g) => g.statuses.includes(r.session.status)));
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  const daysBack = (n: number) => {
    const d = new Date(today);
    d.setDate(d.getDate() - n);
    return d.getTime();
  };
  const [t0, t1, t6] = [today.getTime(), daysBack(1), daysBack(6)];
  const sections: SessionSection[] = [
    { id: 'today', label: 'Today', rows: [] },
    { id: 'yesterday', label: 'Yesterday', rows: [] },
    { id: 'week', label: 'Last 7 days', rows: [] },
    { id: 'older', label: 'Older', rows: [] },
  ];
  for (const r of rest.sort((a, b) => bySession(a.session, b.session))) {
    const t = r.session.lastActivity;
    sections[t >= t0 ? 0 : t >= t1 ? 1 : t >= t6 ? 2 : 3].rows.push(r);
  }
  return [...attention, ...sections].filter((s) => s.rows.length > 0);
}

interface SessionsViewProps {
  pinned: SessionItem[];
  sections: SessionSection[];
  now: number;
  selectedKey: string | null;
  domId: (row: SessionItem) => string;
  onSelect: (row: SessionItem) => void;
  onOpen: (row: SessionItem) => void;
  /** Right-click on a row. */
  onContextMenu: (row: SessionItem) => void;
  /** Moves a pinned session to insertion point `index` among the pinned ones showing. */
  onMovePinned: (key: string, index: number) => void;
  /** Archived sessions, listed under a fold at the end. */
  archived: SessionItem[];
  showArchived: boolean;
  onToggleArchived: () => void;
  /** Todos a session is linked to, for its chip. */
  todosFor?: (sessionKey: string) => Todo[] | undefined;
  onShowTodo?: (todo: Todo) => void;
}

export function SessionsView(props: SessionsViewProps) {
  const { pinned, sections, now, selectedKey, domId, onSelect, onOpen, onContextMenu, onMovePinned, archived, showArchived, onToggleArchived, todosFor, onShowTodo } = props;

  const row = (r: SessionItem, drag?: PinDrag) => (
    <AgentRow
      key={r.key}
      lane={r.lane}
      session={r.session}
      domId={domId(r)}
      now={now}
      selected={r.key === selectedKey}
      onSelect={() => onSelect(r)}
      onOpen={() => onOpen(r)}
      pinned={drag != null}
      onContextMenu={() => onContextMenu(r)}
      todos={todosFor?.(r.session.key)}
      onShowTodo={onShowTodo}
      {...drag}
    />
  );

  return (
    <>
      {pinned.length > 0 && <PinnedSection items={pinned} keyOf={(r) => r.session.key} onMove={onMovePinned} row={row} />}
      {sections.map((s) => (
        <section key={s.id} className="ca-bayblock">
          {s.bay ? <BayHeader bay={s.bay} count={s.rows.length} /> : <SectionHeader label={s.label} count={s.rows.length} />}
          <div className="ca-bay-lanes">{s.rows.map((r) => row(r))}</div>
        </section>
      ))}
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
          {showArchived && <div className="ca-bay-lanes">{archived.map((r) => row(r))}</div>}
        </section>
      )}
    </>
  );
}
