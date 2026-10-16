// What a card dropped on the Kanban view does (ui/KanbanView.tsx): the move each column stands
// for. A todo goes back to To do, starts (the quick panel, as ⌘N does), is marked ready for
// review or sent back from it, or is marked done; reordered within To do, it keeps your order. A
// session on no todo can be marked done, or dropped on a todo's card to join it. What a column
// works out for itself it keeps: a todo In review because its PR is, or a session's PR, isn't
// moved by hand. The columns themselves are boardLists.ts `kanbanColumns`.

import type { KanbanCard, KanbanColumn, KanbanColumnId } from './boardLists';
import type { TodoItem, TodoOp, TodoPatch } from './todos';
import type { SessionItem } from './ui/SessionsView';

/** What letting a card go somewhere does. */
export type KanbanMove =
  | { kind: 'order'; index: number }
  | { kind: 'status'; status: 'todo' | 'in-progress' | 'done' }
  | { kind: 'start' }
  | { kind: 'review'; on: boolean }
  | { kind: 'done' }
  | { kind: 'link'; todo: string };

/** A place a card can be let go over, as the drag shows it (useRowDrag.tsx `DropTarget`). */
export interface KanbanDrop {
  id: string;
  icon?: string;
  word?: string;
  /** The column it lands in, for the view to light it. */
  column: KanbanColumnId;
  /** The card it's dropped on, when that's what takes it. */
  onto?: string;
  move: KanbanMove;
}

/**
 * What `card`, in column `from`, does dropped at column `to`: on the card `onto` there, or at
 * insertion point `index` among To do's cards. Null where it does nothing: back where it was in a
 * column that keeps no order of yours, or a move a column works out for itself.
 */
export function kanbanDrop(card: KanbanCard, from: KanbanColumnId, to: KanbanColumnId, onto: KanbanCard | null, index: number | null): KanbanDrop | null {
  if (card.kind === 'agent') return sessionDrop(from, to, onto);
  return todoDrop(card, from, to, index);
}

/** The insertion point among cards whose tops and bottoms are `spans`, top to bottom, that a drop
 *  at height `y` lands at: before the first card whose middle is below it, else after them all. */
export function insertionAt(y: number, spans: readonly { top: number; bottom: number }[]): number {
  const i = spans.findIndex((s) => y < (s.top + s.bottom) / 2);
  return i < 0 ? spans.length : i;
}

function sessionDrop(from: KanbanColumnId, to: KanbanColumnId, onto: KanbanCard | null): KanbanDrop | null {
  if (onto?.kind === 'todo' && onto.todo.status !== 'done') {
    return { id: `link:${onto.key}`, icon: 'link', word: 'Add to todo', column: to, onto: onto.key, move: { kind: 'link', todo: onto.todo.id } };
  }
  if (to === 'done' && from !== 'done') return { id: 'done', icon: 'check', word: 'Mark as done', column: 'done', move: { kind: 'done' } };
  return null;
}

function todoDrop(card: TodoItem, from: KanbanColumnId, to: KanbanColumnId, index: number | null): KanbanDrop | null {
  const t = card.todo;
  const at = (move: KanbanMove, icon?: string, word?: string): KanbanDrop => ({ id: `${to}:${move.kind}${index != null && move.kind === 'order' ? `:${index}` : ''}`, icon, word, column: to, move });
  switch (to) {
    case 'todo':
      if (from === 'todo') return index == null ? null : at({ kind: 'order', index });
      return at({ kind: 'status', status: 'todo' }, 'arrow-back-up', from === 'done' ? 'Reopen' : 'Back to to do');
    case 'progress':
      if (from === 'todo') return at({ kind: 'start' }, 'player-play', 'Start session');
      if (from === 'done') return at({ kind: 'status', status: 'in-progress' }, 'arrow-back-up', 'Reopen');
      // Only a mark of yours or its agent's takes it out of review; its PR keeps it there.
      return from === 'review' && t.review ? at({ kind: 'review', on: false }, 'arrow-back-up', 'Back to in progress') : null;
    case 'review':
      return from === 'review' ? null : at({ kind: 'review', on: true }, 'eye', 'Ready for review');
    case 'done':
      return from === 'done' ? null : at({ kind: 'status', status: 'done' }, 'check', 'Done');
  }
}

/** Where the arrows go from the card `key`: up and down its column, left and right to the card as
 *  far down the next column that has any, or its last; Done only while it's open. From no card
 *  listed, the first. */
export function kanbanStep(columns: KanbanColumn[], key: string | null, dir: 'up' | 'down' | 'left' | 'right', showDone: boolean): string | null {
  const cols = columns.filter((c) => c.cards.length > 0 && (c.id !== 'done' || showDone));
  const ci = cols.findIndex((c) => c.cards.some((x) => x.key === key));
  if (ci < 0) return cols[0]?.cards[0]?.key ?? null;
  const cards = cols[ci].cards;
  const ri = cards.findIndex((x) => x.key === key);
  if (dir === 'up' || dir === 'down') return cards[Math.max(0, Math.min(cards.length - 1, ri + (dir === 'up' ? -1 : 1)))].key;
  const next = cols[ci + (dir === 'left' ? -1 : 1)];
  return next ? next.cards[Math.min(ri, next.cards.length - 1)].key : key;
}

/** What the window does a move with. */
export interface KanbanActs {
  /** A change to todos, through the window's store, which shows it at once. */
  run: (op: TodoOp) => void;
  /** Start session…: the quick panel on the todo, as ⌘N on it opens it. */
  start: (item: TodoItem) => void;
  /** Mark as done on a session, as its menu does it: its warnings and its Undo with it. */
  markDone: (item: SessionItem) => void;
  /** Moves a todo to insertion point `index` among To do's todos. */
  order: (id: string, index: number) => void;
  /** Says what a move did, with the changes that put it back. */
  said: (message: string, undo: TodoOp[]) => void;
}

/** Does what `drop` says to `card`. A todo's status and review are put back by the toast's Undo,
 *  its wait with them. */
export function applyKanbanDrop(card: KanbanCard, drop: KanbanDrop, acts: KanbanActs): void {
  const m = drop.move;
  if (card.kind === 'agent') {
    if (m.kind === 'done') acts.markDone(card);
    else if (m.kind === 'link') acts.run({ op: 'link', id: m.todo, session: card.session.key, linked: true });
    return;
  }
  const t = card.todo;
  const title = `“${t.title || 'Untitled'}”`;
  const back: TodoOp[] = [{ op: 'update', id: t.id, patch: { status: t.status, waiting: t.waiting ?? null, review: t.review ?? null } }];
  const update = (patch: TodoPatch, message: string) => {
    acts.run({ op: 'update', id: t.id, patch });
    acts.said(message, back);
  };
  switch (m.kind) {
    case 'order':
      return acts.order(t.id, m.index);
    case 'start':
      return acts.start(card);
    case 'status':
      return update({ status: m.status }, m.status === 'done' ? `Marked ${title} done.` : t.status === 'done' ? `Reopened ${title}.` : `Moved ${title} back to To do.`);
    case 'review':
      // A done todo goes back under way to be reviewed.
      if (!m.on) return update({ review: null }, `${title} is back in progress.`);
      return update({ ...(t.status === 'done' ? { status: 'in-progress' as const } : {}), review: { note: '', since: 0 } }, `${title} is ready for review.`);
  }
}
