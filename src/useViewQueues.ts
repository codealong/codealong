// The main window's ways through what waits on you, one for each view that has an order to go
// through (viewQueue.ts has the rules): Sessions' (App.tsx, the one a session's panel answers
// through), and here PRs' and the todos' of Todo and Kanban, each a `useWaitingQueue` over the rows
// its list shows, so their keys and steps mean what they mean in Sessions, the notch and the
// waiting window. What the strip over a panel goes by is `strip`, and what Next and Back (⌘] ⌘[,
// mainKeys.ts) do in the view shown is `step`: on through the view's own, from the first when what
// the panel shows isn't one of them, and once they've run out, on to the view where the next kind
// in your order waits.

import { useMemo } from 'react';
import type { ListRow } from './boardLists';
import { inOrder, queueKinds, type QueueAction, type Reads } from './queue';
import { queueReads, reviewSince, type ReviewRow } from './quick';
import type { Board, Session } from './types';
import type { Queue } from './ui/SessionPanel';
import { leftOf, useWaitingQueue, type WaitingQueue } from './useWaitingQueue';
import { elsewhereOffer, prShown, prWaitingRows, TODO_READS, todoShown, todoWaitingRows, viewWaits, type QueueView, type TodoQueueRow } from './viewQueue';

/** What the window's views need of it. */
export interface ViewQueuesInput<S extends { key: string; session: Session }> {
  view: string;
  selected: ListRow | null;
  board: Board | null;
  project: string;
  now: number;
  /** What PRs and Todo list, and what Kanban does. */
  prListed: readonly ListRow[];
  todoListed: readonly ListRow[];
  kanbanListed: readonly ListRow[];
  /** Sessions' own: the sessions that wait on you, how it reads them, and its way through them. */
  sessions: { waiting: readonly S[]; reads: Reads<S>; queue: WaitingQueue<boolean, S>; show: (row: S) => void };
  setView: (view: QueueView) => void;
  select: (key: string) => void;
}

export interface ViewQueues {
  prs: WaitingQueue<undefined, ReviewRow>;
  todos: WaitingQueue<undefined, TodoQueueRow>;
  /** What the strip over the panel goes by in `view`: undefined where it has no order. */
  strip: (view: string) => Queue | undefined;
  /** Next or Back in the view shown, from its keys or its strip; false where it isn't one with
   *  an order (the canvas, Schedules). */
  step: (action: Extract<QueueAction, 'next' | 'back'>) => boolean;
}

export function useViewQueues<S extends { key: string; session: Session }>(o: ViewQueuesInput<S>): ViewQueues {
  const { view, selected, board, project, now, sessions } = o;
  const order = queueKinds(board?.queue?.order);
  const todoView = view === 'kanban' ? 'kanban' : 'todo';

  const prWaiting = useMemo(() => prWaitingRows(o.prListed, board, project), [o.prListed, board, project]);
  const prReads = queueReads(board ?? {});
  const prs = useWaitingQueue<ReviewRow>({
    waiting: prWaiting,
    reads: prReads,
    shown: view === 'prs' && selected?.kind === 'pr' ? prShown(selected.pr, prWaiting.some((r) => r.pr.key === selected.pr.key) ? reviewSince(selected.pr) : undefined) : null,
    show: (row) => o.select(`pr:${row.pr.key}`),
    leftStays: true,
  });

  const todoQueue = board?.todoQueue;
  const todoListed = view === 'kanban' ? o.kanbanListed : o.todoListed;
  const todoWaiting = useMemo(() => todoWaitingRows(todoListed, todoQueue, now), [todoListed, todoQueue, now]);
  const todoReads = inOrder(TODO_READS, order);
  const listedTodo = selected?.kind === 'todo' ? todoWaiting.find((r) => r.item.todo.id === selected.todo.id) : undefined;
  const todos = useWaitingQueue<TodoQueueRow>({
    waiting: todoWaiting,
    reads: todoReads,
    shown: (view === 'todo' || view === 'kanban') && selected?.kind === 'todo' ? todoShown(selected, listedTodo?.waiting.since) : null,
    show: (row) => o.select(row.item.key),
    leftStays: true,
  });

  // What waits in each view not yet dealt with there, for what Next offers once a view's own have
  // run out. The todos are Todo's, as listed there, whichever view is shown but Kanban.
  const elsewhere = [viewWaits('sessions', sessions.queue.pending, sessions.reads), viewWaits('prs', prs.pending, prReads), viewWaits('todo', todos.pending, todoReads)];

  /** Goes to `to` and shows the first that waits there. */
  const goTo = (to: QueueView) => {
    const s = to === 'sessions' ? sessions.queue.first : null;
    const pr = to === 'prs' ? prs.first : null;
    const todo = to === 'todo' ? todos.first : null;
    if (!s && !pr && !todo) return;
    o.setView(to);
    if (s) sessions.show(s);
    else if (pr) o.select(`pr:${pr.pr.key}`);
    else if (todo) o.select(todo.item.key);
  };

  const viewOf = (at: string): QueueView | null => (at === 'sessions' || at === 'prs' ? at : at === 'todo' || at === 'kanban' ? todoView : null);

  /** What the strip over a panel in `at` goes by: on from what it shows through its view's own,
   *  or from their first when it shows none of them; with none left, on to where the next kind
   *  waits. */
  const strip = (at: string): Queue | undefined => {
    const v = viewOf(at);
    if (!v) return undefined;
    const base = v === 'sessions' ? fromFirst(sessions.queue, sessions.reads, sessions.show) : v === 'prs' ? fromFirst(prs, prReads, (r) => o.select(`pr:${r.pr.key}`)) : fromFirst(todos, todoReads, (r) => o.select(r.item.key));
    if (base.next) return base;
    // Whether anything of its own waits: the one shown, where it does, else what's pending there.
    const q = v === 'sessions' ? sessions.queue : v === 'prs' ? prs : todos;
    const offer = elsewhereOffer(v, elsewhere, q.queue ? Number(q.queue.waiting !== false) : q.pending.length);
    // Sending or marking seen doesn't go on to another view: only Next does.
    return offer ? { ...base, next: { title: offer.title }, left: undefined, ready: undefined, waiting: false, elsewhere: offer.words, onNext: () => goTo(offer.view) } : base;
  };

  const step = (action: 'next' | 'back'): boolean => {
    const v = viewOf(view);
    if (!v) return false;
    // Sessions' go on with the cursor in the next one's box, as its keys always have.
    const run = (a: 'next' | 'back') => (v === 'sessions' ? sessions.queue.run(a, true) : v === 'prs' ? prs.run(a) : todos.run(a));
    const shown = v === 'sessions' ? sessions.queue.queue : v === 'prs' ? prs.queue : todos.queue;
    if (action === 'back' || shown?.next) return run(action) || true;
    const s = strip(view);
    if (s?.next) s.onNext();
    return true;
  };

  return { prs, todos, strip, step };
}

/** A view's way on as the strip has it: from what it shows, or with nothing of its own shown (a
 *  worktree's panel in Sessions, a todo that doesn't wait), from the first that waits there. */
function fromFirst<T, H>(q: WaitingQueue<H, T>, reads: Reads<T>, show: (row: T) => void): Queue {
  if (q.queue) return q.queue;
  const first = q.first;
  // What Next can reach: what's been dealt with here and is still listed isn't counted.
  return { next: first ? { title: reads.titleOf(first) } : null, ...leftOf(q.pending, reads), waiting: false, onNext: () => first && show(first) };
}
