// What Next goes through in each of the main window's views, as pure functions with tests: the
// strip over every panel has Next (SessionPanel.tsx `QueueStrip`), and it stays within the view
// you're in, so the selection visibly moves down its list. Sessions goes through the sessions that
// wait on you (boardLists.ts `waitingRows`), PRs through the PRs it lists that wait on your review,
// Todo and Kanban through the todos they list that wait on you, due ones and then ready ones
// (barfeed.rs `todo_queue`, the notch's too), each by queue.ts's rules (useWaitingQueue.ts). Once a
// view's run out, Next offers the next kind in your order (`[queue] order`) that waits in another
// view (`elsewhereOffer`): "Nothing else waits in PRs · next: 2 sessions in Sessions". The canvas
// and Schedules have no order to go through, and their Next stays dimmed, saying so (`NOWHERE`).

import type { ListRow } from './boardLists';
import { reviewRows, type ReviewRow } from './quick';
import type { Reads, Waits } from './queue';
import type { TodoItem } from './todos';
import type { BarTodo, Board, PrItem, TodoQueue } from './types';

/** A view the main window can go through with Next. */
export type QueueView = 'sessions' | 'prs' | 'todo' | 'kanban';

/** What a view whose Next goes nowhere says of it. */
export const NOWHERE: Record<string, string> = {
  canvas: 'The canvas has no order to go through: Next is in Sessions, PRs and Todo',
  schedules: 'Schedules don’t wait on you: Next is in Sessions, PRs and Todo',
};

/** The PRs a view lists that wait on your review, as the floating bar and the waiting window have
 *  them (quick.ts `reviewRows`, which leaves them all out while `[queue]` does): only the ones
 *  listed, so Next goes down the list as the filters have it. */
export function prWaitingRows(listed: readonly ListRow[], board: Board | null, project: string): ReviewRow[] {
  if (!board) return [];
  const shown = new Set(listed.flatMap((r) => (r.kind === 'pr' ? [r.pr.key] : [])));
  return reviewRows(board, project).filter((r) => shown.has(r.pr.key));
}

/** A todo the Todo or Kanban view lists that waits on you. */
export interface TodoQueueRow {
  /** Its row in the list. */
  item: TodoItem;
  /** How the board's queue has it: since when it waits, and its place in your order. */
  waiting: BarTodo;
  waits: 'due' | 'ready';
}

/** The todos a view lists that wait on you, as the board has them (`Board.todoQueue`): the due
 *  ones, then the ready ones. Only the ones listed, as the list has them now: one this window
 *  marked done, archived or snoozed (todos.ts `useTodoStore`, ahead of the board) has left. */
export function todoWaitingRows(listed: readonly ListRow[], queue: TodoQueue | undefined, now: number): TodoQueueRow[] {
  if (!queue) return [];
  const items = new Map(listed.flatMap((r) => (r.kind === 'todo' ? [[r.todo.id, r] as const] : [])));
  const rows = (list: BarTodo[], waits: TodoQueueRow['waits']) =>
    list.flatMap((waiting) => {
      const item = items.get(waiting.id);
      const t = item?.todo;
      return item && t && t.status !== 'done' && !t.archivedAt && !((t.snoozedUntil ?? 0) > now) ? [{ item, waiting, waits }] : [];
    });
  return [...rows(queue.due, 'due'), ...rows(queue.ready, 'ready')];
}

/** How the queue reads a todo's row: as the notch reads its todos (bar.ts `ROW_READS`). */
export const TODO_READS: Reads<TodoQueueRow> = {
  keyOf: (r) => r.waiting.id,
  sinceOf: (r) => r.waiting.since ?? 0,
  waitsOf: (r) => r.waits,
  raisedOf: () => undefined,
  finishedOf: () => false,
  orderOf: (r) => r.waiting.order,
  titleOf: (r) => r.item.todo.title || 'Untitled todo',
};

/** The PR shown in PRs, as the queue takes it: there's nothing to mark seen, so ⌘E goes on. */
export function prShown(pr: PrItem, since: number | undefined) {
  return { key: pr.key, title: `#${pr.number} ${pr.title}`, since, seenable: false, skipsOnSeen: true };
}

/** The todo shown in Todo or Kanban, as the queue takes it. One dealt with that waits again
 *  (opened again, due again) comes back, as in the notch. */
export function todoShown(item: TodoItem, since: number | undefined) {
  return { key: item.todo.id, title: item.todo.title || 'Untitled todo', since, seenable: false, skipsOnSeen: true, comesBack: true };
}

/** Where each kind of what waits on you is gone through. */
const VIEW_OF: Record<Waits, QueueView> = { needs: 'sessions', unread: 'sessions', reviews: 'prs', due: 'todo', ready: 'todo' };
const VIEW_WORD: Record<QueueView, string> = { sessions: 'Sessions', prs: 'PRs', todo: 'Todo', kanban: 'Kanban' };

/** How many of each kind wait in the views, of those not yet dealt with there, and of them how
 *  many sessions were moved to the top and how many PRs are of a repo that puts reviews first,
 *  which go ahead of every kind (queue.ts `byPlace`). */
export type WaitCounts = Partial<Record<Waits | 'raised' | 'firstReviews', number>>;

/** What Next offers once the view `from` has nothing left: the first kind in your order that
 *  waits in another view, and how the strip says it, `own` being how many of the view's own wait
 *  (none: nothing waits there at all). None when nothing waits anywhere else. The sessions' two
 *  kinds are one offer: Sessions goes through them in your order itself. */
export function elsewhereOffer(from: QueueView, counts: WaitCounts, order: readonly Waits[], own = 0): { view: QueueView; words: string; title: string } | null {
  const ours = (k: Waits) => VIEW_OF[k] === from || (from === 'kanban' && VIEW_OF[k] === 'todo');
  // A session moved to the top comes before everything, then the reviews of a repo that puts
  // them first, then the kinds in your order.
  const ahead: Waits | undefined = !ours('needs') && (counts.raised ?? 0) > 0 ? 'needs' : !ours('reviews') && (counts.firstReviews ?? 0) > 0 ? 'reviews' : undefined;
  const kind = ahead ?? order.find((k) => !ours(k) && (counts[k] ?? 0) > 0);
  if (!kind) return null;
  const view = VIEW_OF[kind];
  const n = view === 'sessions' ? (counts.needs ?? 0) + (counts.unread ?? 0) : (counts[kind] ?? 0);
  const what = view === 'sessions' ? (n === 1 ? 'session' : 'sessions') : view === 'prs' ? (n === 1 ? 'review' : 'reviews') : `${kind} ${n === 1 ? 'todo' : 'todos'}`;
  return { view, words: `Nothing ${own > 0 ? 'else ' : ''}waits in ${VIEW_WORD[from]}`, title: `${n} ${what} in ${VIEW_WORD[view]}` };
}
