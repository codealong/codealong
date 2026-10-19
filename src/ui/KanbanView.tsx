// A board's Kanban (boardLists.ts `kanbanColumns`): its work as cards in its columns, the four
// stages unless it has columns of its own. A card is a todo, with the sessions on it listed under
// its two lines, or a session on no todo. Its face is the canvas's (card/CardFace.tsx); this file
// adds where it goes and what a press, a double-click, a right-click and a drag do, which are a
// list's row's (primitives.tsx `Row`). A card changes column only when its work moves on (started,
// a PR opened, merged, done) or you move it, never with each turn: whose move it is shows in its
// stripe and its place in a column ordered by it. A column that starts folded (Done) is a rail
// until you open it, and a column's right-click changes it. A card dropped on another column, or a
// session's on a todo's card, does what kanban.ts says; one dragged out of the window is pinned to
// the desktop.

import { useMemo, type ReactNode } from 'react';
import { cardLead, NO_COLUMN, type KanbanCard, type KanbanColumn } from '../boardLists';
import type { Stage } from '../boards/model';
import { insertionAt, kanbanDrop, type DropContext, type KanbanDrop } from '../kanban';
import { selectHow, type SelectHow } from '../picks';
import { blockedBy, itemSection, type TodoItem } from '../todos';
import type { Pr, Session, Todo, TodoHint, TodoStatus } from '../types';
import { bySession } from '../lanes';
import { CardFace } from './card/CardFace';
import { SessionLine } from './card/CardPopover';
import type { CardData } from './card/data';
import { cx, Icon, Row } from './primitives';
import { TodoGlyph } from './TodoBits';
import { useRowDrag } from './useRowDrag';
import { cmd } from '../platform';

/** How many of a todo's sessions its card lists, the most urgent first. */
const SESSIONS_SHOWN = 3;

/** A stage column's box, as its todos' glyph draws it. */
const STAGE_GLYPH: Record<Stage, { status: TodoStatus; review?: boolean }> = {
  todo: { status: 'todo' },
  progress: { status: 'in-progress' },
  review: { status: 'in-progress', review: true },
  done: { status: 'done' },
};

const STAGE_EMPTY: Record<Stage, string> = {
  todo: 'Nothing waiting to start.',
  progress: 'Nothing under way.',
  review: 'Nothing waiting on a review.',
  done: 'Nothing done lately.',
};

/** What a card's face is of, by the id the face carries (`data-id`). */
function faceItem(c: KanbanCard): { ref: 'todo' | 'session'; key: string; title: string } {
  return c.kind === 'todo' ? { ref: 'todo', key: c.todo.id, title: c.todo.title } : { ref: 'session', key: c.session.key, title: c.session.title };
}

function faceData(c: KanbanCard, section: ReturnType<typeof itemSection> | null): CardData {
  return c.kind === 'todo' ? { ref: 'todo', todo: c.todo, labels: c.labels, sessions: c.sessions, lane: c.lane, section: section ?? undefined } : { ref: 'session', lane: c.lane, session: c.session };
}

interface KanbanViewProps {
  columns: KanbanColumn[];
  /** Every todo, in any project: what a card waits on can be anywhere. */
  todos: Todo[];
  now: number;
  selectedKey: string | null;
  /** The todos picked together with the selected card. */
  picked: Set<string>;
  domId: (card: KanbanCard) => string;
  /** Whether a column is open: one that starts folded is a rail until it's opened. */
  open: (c: KanbanColumn) => boolean;
  onFold: (c: KanbanColumn) => void;
  onSelect: (card: KanbanCard, how: SelectHow) => void;
  onOpen: (card: KanbanCard) => void;
  onContextMenu: (card: KanbanCard) => void;
  /** A column's right-click: its settings, folding it, moving it, taking it away. */
  onColumnMenu: (c: KanbanColumn) => void;
  /** Adds a column, after the last. */
  onAddColumn: () => void;
  /** A click on a card's PR chip shows the PR in the panel, its card selected; a double-click
   *  opens it on GitHub. */
  onShowPr: (pr: Pr, card: KanbanCard) => void;
  onOpenPr: (pr: Pr) => void;
  /** A card let go over a column or a card, and what that does (kanban.ts). */
  onDrop: (card: KanbanCard, drop: KanbanDrop) => void;
  /** What a drop is worked out against. */
  dropContext: DropContext;
  /** Dragging reorders a column in your order; not while a search or a filter hides some of it. */
  canReorder: boolean;
  /** Whether a card can be put in its place in a column in your order: on a project's own
   *  board, only todos, which keep the Todo view's order. */
  orderable: (card: KanbanCard) => boolean;
  /** Todos Jev says look done, by id. */
  hints?: Record<string, TodoHint>;
  /** The box that adds a todo, at the top of the column a new todo goes in. */
  composer: ReactNode;
  composerColumn: string | null;
}

export function KanbanView(props: KanbanViewProps) {
  const { columns, todos, open, onFold, canReorder, onDrop, dropContext } = props;
  const byFace = useMemo(() => new Map<string, { card: KanbanCard; column: KanbanColumn }>(columns.flatMap((col) => col.cards.map((c) => [`${faceItem(c).ref}:${faceItem(c).key}`, { card: c, column: col }] as const))), [columns]);
  const byKey = useMemo(() => new Map([...byFace.values()].map((x) => [x.card.key, x])), [byFace]);
  const byId = useMemo(() => new Map(columns.map((c) => [c.id, c])), [columns]);

  const drag = useRowDrag<KanbanDrop>({
    desktop: (key) => {
      const c = byKey.get(key)?.card;
      return c ? faceItem(c) : null;
    },
    label: (key) => {
      const c = byKey.get(key)?.card;
      return c ? faceItem(c).title || 'Untitled' : '';
    },
    // The column under the pointer, the card it's over, and in a column in your order where
    // among its cards.
    dropAt: (key, x, y) => {
      const from = byKey.get(key);
      const at = document.elementFromPoint(x, y);
      const col = at?.closest<HTMLElement>('[data-kb-column]');
      const to = col ? byId.get(col.dataset.kbColumn ?? '') : undefined;
      if (!from || !col || !to) return null;
      const face = at?.closest<HTMLElement>('[data-id]')?.dataset.id;
      const onto = face ? (byFace.get(face)?.card ?? null) : null;
      const ordered = to.column?.sort === 'hand' && canReorder && props.orderable(from.card);
      const spans = ordered ? [...col.querySelectorAll('[data-pin-index]')].map((el) => el.getBoundingClientRect()) : null;
      return kanbanDrop(from.card, from.column, to, onto && onto.key !== key ? onto : null, spans ? insertionAt(y, spans) : null, dropContext);
    },
    onDrop: (key, drop) => {
      const c = byKey.get(key)?.card;
      if (c) onDrop(c, drop);
    },
  });
  const target = drag.dropTarget;

  return (
    <div className={cx('ca-kanban', drag.dragging && 'is-dragging')}>
      {columns.map((col) =>
        !open(col) ? (
          <button
            key={col.id}
            type="button"
            className={cx('ca-kb-rail', target?.column === col.id && 'is-drop')}
            data-kb-column={col.id}
            title={`Show the ${col.cards.length} in ${col.label}`}
            aria-expanded={false}
            onClick={() => onFold(col)}
            onContextMenu={(e) => {
              e.preventDefault();
              props.onColumnMenu(col);
            }}
          >
            <ColumnGlyph stage={col.stage} />
            <span className="ca-kb-rail-label">{col.label}</span>
            <span className="ca-bay-count">{col.cards.length}</span>
          </button>
        ) : (
          <Column key={col.id} column={col} props={props} drag={drag} target={target} onFold={col.column?.folded ? () => onFold(col) : undefined} todos={todos} />
        ),
      )}
      <button type="button" className="ca-kb-add" title="Add a column" aria-label="Add a column" onClick={props.onAddColumn}>
        <Icon name="plus" size={14} />
      </button>
      {drag.ghost}
    </div>
  );
}

function ColumnGlyph({ stage }: { stage: Stage | null }) {
  if (!stage) return <Icon name="layout-kanban" size={11} />;
  const g = STAGE_GLYPH[stage];
  return <TodoGlyph status={g.status} review={g.review} size={11} title="" />;
}

function Column({ column, props, drag, target, onFold, todos }: { column: KanbanColumn; props: KanbanViewProps; drag: ReturnType<typeof useRowDrag<KanbanDrop>>; target: KanbanDrop | null; onFold?: () => void; todos: Todo[] }) {
  const { now, selectedKey, picked, domId, onSelect, onOpen, onContextMenu, onShowPr, onOpenPr, canReorder, orderable, hints, composer, composerColumn } = props;
  const ordered = column.column?.sort === 'hand' && canReorder && column.cards.length > 0;
  const sections = column.cards.map((c) => (c.kind === 'todo' ? itemSection(c, todos) : null));
  // A column in your order lists the rest, then what waits: a line says where Waiting begins.
  const firstWaiting = column.column?.sort === 'hand' ? sections.indexOf('waiting') : -1;
  const dropAt = target?.column === column.id ? target.index : null;
  const lights = target?.column === column.id && !target.onto && target.index == null;
  const empty = column.id === NO_COLUMN ? 'Cards no column takes.' : column.stage ? STAGE_EMPTY[column.stage] : column.column?.rule ? 'Nothing matches.' : 'Drag cards here.';
  return (
    <section className={cx('ca-kb-col', column.stage && `is-${column.stage}`, lights && 'is-drop')} data-kb-column={column.id} aria-label={column.label}>
      <div
        className="ca-bay ca-kb-head"
        role="heading"
        aria-level={3}
        title={column.id === NO_COLUMN ? 'Cards no column takes: give one a rule, or a column of your own, to hold them' : 'Right-click to change the column'}
        onContextMenu={(e) => {
          e.preventDefault();
          if (column.id !== NO_COLUMN) props.onColumnMenu(column);
        }}
      >
        <ColumnGlyph stage={column.stage} />
        <span className="ca-kb-head-name">{column.label}</span>
        <span className="ca-bay-count">{column.cards.length}</span>
        {onFold && (
          <button type="button" className="ca-linkbtn ca-kb-fold" aria-expanded onClick={onFold}>
            Hide
          </button>
        )}
      </div>
      <div className="ca-kb-cards">
        {column.id === composerColumn && composer}
        {column.cards.length === 0 && <div className="ca-kb-empty">{empty}</div>}
        {column.cards.map((c, i) => (
          <Card
            key={c.key}
            card={c}
            section={sections[i]}
            divider={i === firstWaiting ? 'Waiting' : null}
            domId={domId(c)}
            now={now}
            selected={c.key === selectedKey || picked.has(c.key)}
            onto={target?.onto === c.key}
            dropBefore={dropAt === i}
            dropAfter={dropAt === column.cards.length && i === column.cards.length - 1}
            blocked={c.kind === 'todo' ? blockedBy(c.todo, todos) : []}
            hint={c.kind === 'todo' ? hints?.[c.todo.id] : undefined}
            onSelect={(how) => onSelect(c, how)}
            onOpen={() => onOpen(c)}
            onMenu={() => onContextMenu(c)}
            onShowPr={(pr) => onShowPr(pr, c)}
            onOpenPr={onOpenPr}
            drag={drag.rowProps(c.key, ordered && orderable(c) ? i : null, column.cards.length)}
          />
        ))}
      </div>
    </section>
  );
}

interface CardProps {
  card: KanbanCard;
  section: ReturnType<typeof itemSection> | null;
  /** A line over the card, where a part of its column begins. */
  divider: string | null;
  domId: string;
  now: number;
  selected: boolean;
  /** A session being dragged would join this todo. */
  onto: boolean;
  dropBefore: boolean;
  dropAfter: boolean;
  /** The open todos it waits on. */
  blocked: Todo[];
  hint?: TodoHint;
  onSelect: (how: SelectHow) => void;
  onOpen: () => void;
  onMenu: () => void;
  onShowPr: (pr: Pr) => void;
  onOpenPr: (pr: Pr) => void;
  drag: ReturnType<ReturnType<typeof useRowDrag>['rowProps']>;
}

function Card({ card, section, divider, domId, now, selected, onto, dropBefore, dropAfter, blocked, hint, onSelect, onOpen, onMenu, onShowPr, onOpenPr, drag }: CardProps) {
  const lead = cardLead(card);
  // A todo's card takes its most urgent session's stripe, as a session's own card has it.
  const stripe = card.kind === 'todo' && lead ? `st-${lead.status}` : null;
  return (
    <>
      {divider && <div className="ca-kb-divider">{divider}</div>}
      <Row
        id={domId}
        selected={selected}
        className={cx('ca-kb-cardrow', onto && 'is-onto', dropBefore && 'is-drop-before', dropAfter && 'is-drop-after')}
        drag={drag}
        // ⌘ adds a todo to the ones picked or leaves it out, ⇧ picks the todos up to it. A
        // right-click leaves the selection alone: on a picked card, the menu is for all of them.
        onSelect={(e) => onSelect(selectHow(e))}
        onOpen={(e) => {
          if (!cmd(e) && !e.shiftKey) onOpen();
        }}
        onMenu={onMenu}
      >
        <CardFace item={faceItem(card)} data={faceData(card, section)} now={now} className={cx('is-kanban', stripe)} onPrShow={onShowPr} onPrOpen={onOpenPr}>
          {card.kind === 'todo' && <TodoCardMore item={card} blocked={blocked} hint={hint} now={now} />}
        </CardFace>
      </Row>
    </>
  );
}

/** Under a todo's two lines: what it waits on or why it's in review, and its sessions. */
function TodoCardMore({ item, blocked, hint, now }: { item: TodoItem; blocked: Todo[]; hint?: TodoHint; now: number }) {
  const t = item.todo;
  const open = t.status !== 'done';
  const note = !open
    ? null
    : t.waiting
      ? t.waiting.note
        ? `Waiting for ${t.waiting.note}`
        : 'Marked as waiting'
      : blocked.length > 0
        ? blocked.length === 1
          ? `Waiting on ${blocked[0].title || 'Untitled'}`
          : `Waiting on ${blocked.length} todos`
        : t.review
          ? t.review.note
            ? `Review ${t.review.note}`
            : 'Ready for review'
          : hint
            ? 'Looks done'
            : null;
  const sessions: Session[] = item.sessions.map((p) => p.session).sort(bySession);
  return (
    <>
      {note && <div className={cx('ca-kb-note', hint && !t.waiting && !t.review && blocked.length === 0 && 'is-done')}>{note}</div>}
      {sessions.length > 0 && (
        <div className="ca-kb-sessions">
          {sessions.slice(0, SESSIONS_SHOWN).map((s) => (
            <SessionLine key={s.key} s={s} now={now} />
          ))}
          {sessions.length > SESSIONS_SHOWN && <div className="ca-kb-more">+{sessions.length - SESSIONS_SHOWN} more</div>}
        </div>
      )}
    </>
  );
}
