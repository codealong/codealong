// What a card dropped on a board's Kanban does (ui/KanbanView.tsx): whatever lands it in that
// column (boards/rules.ts `columnOf`), or nothing. A stage column moves a todo's work on as its
// stage says: back to To do, started (the quick panel, as ⌘N does), marked ready for review or
// sent back from it, or done; a session can be marked done there. A label column gives a todo its
// label, and one it leaves takes its own away. A column of your own puts the card there, and when a
// rule column holds it, undoes what holds it first (Done reopens it). A session dropped on a todo's
// card joins that todo. What a column works out for itself it keeps: a todo In review because its
// PR is can't be dragged out by hand, nor a session given a label. Within a column in your order a
// card keeps the place you drop it at. The columns are boardLists.ts `kanbanColumns`.

import type { KanbanCard, KanbanColumn } from './boardLists';
import { refId } from './canvas/model';
import type { BoardColumn, Stage } from './boards/model';
import { columnOf, todoStage, todoSubject, type Subject } from './boards/rules';
import { applyOp, type TodoItem, type TodoOp, type TodoPatch } from './todos';
import type { Todo } from './types';
import type { SessionItem } from './ui/SessionsView';

/** One thing letting a card go somewhere does. */
export type KanbanMove =
  | { kind: 'order'; index: number }
  | { kind: 'status'; status: 'todo' | 'in-progress' | 'done' }
  | { kind: 'start' }
  | { kind: 'review'; on: boolean }
  | { kind: 'done' }
  | { kind: 'link'; todo: string }
  | { kind: 'labels'; add: string[]; remove: string[] }
  | { kind: 'place'; column: string };

/** A place a card can be let go over, as the drag shows it (useRowDrag.tsx `DropTarget`). */
export interface KanbanDrop {
  id: string;
  icon?: string;
  word?: string;
  /** The column it lands in, for the view to light it. */
  column: string;
  /** The card it's dropped on, when that's what takes it. */
  onto?: string;
  /** Where among the column's cards it goes, in a column in your order. */
  index: number | null;
  moves: KanbanMove[];
}

/** What a drop is worked out against. */
export interface DropContext {
  /** The board's columns, in order. */
  columns: BoardColumn[];
  placed: Record<string, string> | undefined;
  subjects: Map<string, Subject>;
  /** Every todo: what a todo waits on decides its stage. */
  todos: Todo[];
  now: number;
}

/** A card's id, as boards name it. */
export function kanbanCardId(c: KanbanCard): string {
  return c.kind === 'todo' ? refId('todo', c.todo.id) : refId('session', c.session.key);
}

/**
 * What `card`, in column `from`, does dropped at column `to`: on the card `onto` there, or at
 * insertion point `index` among the column's cards (given only where they keep your order). Null
 * where it does nothing: back where it was in a column that keeps no order of yours, or where no
 * change would land it there.
 */
export function kanbanDrop(card: KanbanCard, from: KanbanColumn, to: KanbanColumn, onto: KanbanCard | null, index: number | null, ctx: DropContext): KanbanDrop | null {
  if (card.kind === 'agent') return sessionDrop(card, from, to, onto, index, ctx);
  return todoDrop(card, from, to, index, ctx);
}

/** The insertion point among cards whose tops and bottoms are `spans`, top to bottom, that a drop
 *  at height `y` lands at: before the first card whose middle is below it, else after them all. */
export function insertionAt(y: number, spans: readonly { top: number; bottom: number }[]): number {
  const i = spans.findIndex((s) => y < (s.top + s.bottom) / 2);
  return i < 0 ? spans.length : i;
}

const dropId = (to: KanbanColumn, moves: KanbanMove[], index: number | null) => `${to.id}:${moves.map((m) => m.kind).join('+')}${index != null ? `:${index}` : ''}`;

function sessionDrop(card: SessionItem, from: KanbanColumn, to: KanbanColumn, onto: KanbanCard | null, index: number | null, ctx: DropContext): KanbanDrop | null {
  if (onto?.kind === 'todo' && onto.todo.status !== 'done') {
    return { id: `link:${onto.key}`, icon: 'link', word: 'Add to todo', column: to.id, onto: onto.key, index: null, moves: [{ kind: 'link', todo: onto.todo.id }] };
  }
  if (from.id === to.id) return index == null ? null : { id: dropId(to, [], index), column: to.id, index, moves: [{ kind: 'order', index }] };
  if (to.stage === 'done') return { id: `${to.id}:done`, icon: 'check', word: 'Mark as done', column: to.id, index: null, moves: [{ kind: 'done' }] };
  // Only a column of your own takes a session by hand, and only where no rule column holds it.
  const subject = ctx.subjects.get(kanbanCardId(card));
  if (!to.column || to.column.rule || !subject) return null;
  if (columnOf(subject, ctx.columns, { ...ctx.placed, [subject.card]: to.id }, ctx.now) !== to.id) return null;
  const moves: KanbanMove[] = [{ kind: 'place', column: to.id }, ...(index != null ? [{ kind: 'order' as const, index }] : [])];
  return { id: dropId(to, moves, index), icon: 'arrow-right', word: `Move to ${to.label}`, column: to.id, index, moves };
}

/** A step from one stage to another: what does it, and the change to the todo it comes to, for
 *  working out where the todo lands. */
interface Step {
  moves: KanbanMove[];
  patch: TodoPatch;
  icon: string;
  word: string;
}

function stageStep(item: TodoItem, from: Stage, to: Stage): Step | null {
  const t = item.todo;
  switch (to) {
    case 'todo':
      return { moves: [{ kind: 'status', status: 'todo' }], patch: { status: 'todo' }, icon: 'arrow-back-up', word: from === 'done' ? 'Reopen' : 'Back to to do' };
    case 'progress':
      if (from === 'todo') return { moves: [{ kind: 'start' }], patch: { status: 'in-progress' }, icon: 'player-play', word: 'Start session' };
      if (from === 'done') return { moves: [{ kind: 'status', status: 'in-progress' }], patch: { status: 'in-progress' }, icon: 'arrow-back-up', word: 'Reopen' };
      // Only a mark of yours or its agent's takes it out of review; its PR keeps it there.
      return t.review ? { moves: [{ kind: 'review', on: false }], patch: { review: null }, icon: 'arrow-back-up', word: 'Back to in progress' } : null;
    case 'review':
      return { moves: [{ kind: 'review', on: true }], patch: { ...(t.status === 'done' ? { status: 'in-progress' as const } : {}), review: { note: '', since: 0 } }, icon: 'eye', word: 'Ready for review' };
    case 'done':
      return { moves: [{ kind: 'status', status: 'done' }], patch: { status: 'done' }, icon: 'check', word: 'Done' };
  }
}

function todoDrop(card: TodoItem, from: KanbanColumn, to: KanbanColumn, index: number | null, ctx: DropContext): KanbanDrop | null {
  if (from.id === to.id) return index == null ? null : { id: dropId(to, [], index), column: to.id, index, moves: [{ kind: 'order', index }] };
  if (!to.column) return null;
  const t = card.todo;
  const rule = to.column.rule ?? null;
  const moves: KanbanMove[] = [];
  let patch: TodoPatch = {};
  let icon = 'arrow-right';
  let word = `Move to ${to.label}`;
  const stage = todoStage(card, ctx.todos);
  const step = (s: Step | null) => {
    if (!s) return false;
    moves.push(...s.moves);
    patch = { ...patch, ...s.patch };
    icon = s.icon;
    word = s.word;
    return true;
  };
  // What the column asks for: a stage, a label.
  if (rule?.stages?.length && !rule.stages.includes(stage) && !step(stageStep(card, stage, rule.stages[0]))) return null;
  const added: string[] = [];
  const removed: string[] = [];
  if (rule?.labels?.length && !rule.labels.some((l) => t.labels.includes(l))) added.push(rule.labels[0]);
  const place = rule ? null : to.id;
  const lands = () => {
    const labels = [...t.labels.filter((l) => !removed.includes(l)), ...added.filter((l) => !t.labels.includes(l))];
    // As the todo store will have it: what a status or a review mark does to the rest is its own.
    const todo = applyOp({ todos: [t], labels: [] }, { op: 'update', id: t.id, patch: { ...patch, labels } }, ctx.now).todos[0] ?? t;
    const placed = place ? { ...ctx.placed, [refId('todo', t.id)]: place } : ctx.placed;
    return columnOf(todoSubject({ ...card, todo }, ctx.todos), ctx.columns, placed, ctx.now);
  };
  // What holds it in another column: a label column it leaves gives up its label, and a stage
  // column it leaves for one of your own sends its work back to To do.
  for (let tries = 0; tries < ctx.columns.length && lands() !== to.id; tries++) {
    const holder = ctx.columns.find((c) => c.id === lands());
    if (holder?.rule?.labels?.some((l) => t.labels.includes(l) && !removed.includes(l))) removed.push(...holder.rule.labels.filter((l) => t.labels.includes(l)));
    else if (holder?.rule?.stages?.length && !rule && !moves.some((m) => m.kind === 'status' && m.status === 'todo')) {
      if (!step(stageStep(card, stage, 'todo'))) return null;
      icon = 'arrow-right';
      word = `Move to ${to.label}`;
    } else return null;
  }
  if (lands() !== to.id) return null;
  if (added.length || removed.length) {
    moves.push({ kind: 'labels', add: added, remove: removed });
    if (moves.length === 1) word = added.length && !removed.length ? `Add to ${to.label}` : `Move to ${to.label}`;
  }
  if (place) moves.push({ kind: 'place', column: place }, ...(index != null ? [{ kind: 'order' as const, index }] : []));
  if (moves.length === 0) return null;
  return { id: dropId(to, moves, place ? index : null), icon, word, column: to.id, index: place ? index : null, moves };
}

/** Where the arrows go from the card `key`: up and down its column, left and right to the card as
 *  far down the next column that has any, or its last; a folded column only while it's open. From
 *  no card listed, the first. */
export function kanbanStep(columns: KanbanColumn[], key: string | null, dir: 'up' | 'down' | 'left' | 'right', open: (c: KanbanColumn) => boolean): string | null {
  const cols = columns.filter((c) => c.cards.length > 0 && open(c));
  const ci = cols.findIndex((c) => c.cards.some((x) => x.key === key));
  if (ci < 0) return cols[0]?.cards[0]?.key ?? null;
  const cards = cols[ci].cards;
  const ri = cards.findIndex((x) => x.key === key);
  if (dir === 'up' || dir === 'down') return cards[Math.max(0, Math.min(cards.length - 1, ri + (dir === 'up' ? -1 : 1)))].key;
  const next = cols[ci + (dir === 'left' ? -1 : 1)];
  return next ? next.cards[Math.min(ri, next.cards.length - 1)].key : key;
}

/** A column's cards, by id, with `card` put at insertion point `index` among them as they're shown
 *  (where `card` may be already). */
export function columnWith(column: readonly string[], card: string, index: number): string[] {
  const from = column.indexOf(card);
  const rest = column.filter((c) => c !== card);
  rest.splice(Math.max(0, Math.min(rest.length, from >= 0 && index > from ? index - 1 : index)), 0, card);
  return rest;
}

/** `all` with the ids `seq` holds put in its places in `seq`'s order, and the rest where they
 *  were: a column's new order, put back into the order of everything. */
export function inSlots(all: readonly string[], seq: readonly string[]): string[] {
  const present = seq.filter((k) => all.includes(k));
  const slots = new Set(present);
  let i = 0;
  return all.map((k) => (slots.has(k) ? present[i++] : k));
}

/** A board's own order with a column's put first, the order the rest keep among themselves. */
export function boardOrderWith(order: readonly string[], seq: readonly string[]): string[] {
  return [...seq, ...order.filter((c) => !seq.includes(c))];
}

/** What the window does a move with. */
export interface KanbanActs {
  /** A change to todos, through the window's store, which shows it at once. */
  run: (op: TodoOp) => void;
  /** Start session…: the quick panel on the todo, as ⌘N on it opens it. */
  start: (item: TodoItem) => void;
  /** Mark as done on a session, as its menu does it: its warnings and its Undo with it. */
  markDone: (item: SessionItem) => void;
  /** Moves a card to insertion point `index` among its column's cards, in your order. */
  order: (card: KanbanCard, column: string, index: number) => void;
  /** Puts a card in a column of your own, or leaves it to the rules. */
  place: (card: KanbanCard, column: string | null) => void;
  /** Says what a move did, with what puts it back. */
  said: (message: string, undo: () => void) => void;
}

/** Does what `drop` says to `card`. A todo's status, review, wait, labels and place are put back
 *  by the toast's Undo. */
export function applyKanbanDrop(card: KanbanCard, drop: KanbanDrop, acts: KanbanActs, placedBefore: string | null): void {
  const placing = drop.moves.find((m): m is Extract<KanbanMove, { kind: 'place' }> => m.kind === 'place');
  const ordering = drop.moves.find((m): m is Extract<KanbanMove, { kind: 'order' }> => m.kind === 'order');
  if (card.kind === 'agent') {
    for (const m of drop.moves) {
      if (m.kind === 'done') acts.markDone(card);
      else if (m.kind === 'link') acts.run({ op: 'link', id: m.todo, session: card.session.key, linked: true });
    }
    if (placing) acts.place(card, placing.column);
    if (ordering) acts.order(card, drop.column, ordering.index);
    return;
  }
  const t = card.todo;
  const title = `“${t.title || 'Untitled'}”`;
  if (drop.moves.length === 1 && ordering) return acts.order(card, drop.column, ordering.index);
  if (drop.moves.some((m) => m.kind === 'start')) {
    if (placing) acts.place(card, placing.column);
    return acts.start(card);
  }
  const patch: TodoPatch = {};
  for (const m of drop.moves) {
    if (m.kind === 'status') patch.status = m.status;
    else if (m.kind === 'review') {
      // A done todo goes back under way to be reviewed.
      patch.review = m.on ? { note: '', since: 0 } : null;
      if (m.on && t.status === 'done') patch.status = 'in-progress';
    } else if (m.kind === 'labels') patch.labels = [...t.labels.filter((l) => !m.remove.includes(l)), ...m.add.filter((l) => !t.labels.includes(l))];
  }
  if (Object.keys(patch).length > 0) acts.run({ op: 'update', id: t.id, patch });
  if (placing) acts.place(card, placing.column);
  if (ordering) acts.order(card, drop.column, ordering.index);
  const back: TodoOp = { op: 'update', id: t.id, patch: { status: t.status, waiting: t.waiting ?? null, review: t.review ?? null, labels: t.labels } };
  const message =
    patch.status === 'done'
      ? `Marked ${title} done.`
      : patch.status && t.status === 'done'
        ? `Reopened ${title}.`
        : patch.status === 'todo'
          ? `Moved ${title} back to To do.`
          : patch.review
            ? `${title} is ready for review.`
            : patch.review === null
              ? `${title} is back in progress.`
              : `Moved ${title} to ${drop.word?.replace(/^(Move|Add) to /, '') ?? 'its column'}.`;
  acts.said(message, () => {
    acts.run(back);
    if (placing) acts.place(card, placedBefore);
  });
}
