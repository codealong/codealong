import { describe, expect, it } from 'vitest';
import * as fx from './testing/fixtures';
import { byPlace, inOrder, inQueueOrder, placeOf, type Reads, type Waits } from './queue';
import { prRowOf } from './boardLists';
import { todoItem } from './todos';
import type { BarTodo, PrItem, Raised, Todo } from './types';
import { elsewhereOffer, prWaitingRows, TODO_READS, todoWaitingRows, viewWaits, type TodoQueueRow } from './viewQueue';

const NOW = fx.NOW;
const listedTodo = (t: Todo) => todoItem(t, new Map(), new Map(), new Map());
const waiting = (id: string, since: number, order: number): BarTodo => ({ id, title: id, project: 'P', callsign: null, due: null, since, order });

describe('prWaitingRows', () => {
  const asked = fx.prItem({ number: 1, reviewRequested: true });
  const also = fx.prItem({ number: 2, reviewRequested: true });
  const mine = fx.prItem({ number: 3 });
  const row = (pr: PrItem) => prRowOf(pr, new Map(), new Map());

  it('goes through the PRs the list shows that wait on your review', () => {
    const board = fx.board({ prs: [asked, also, mine] });
    expect(prWaitingRows([row(also), row(mine)], board, 'All projects').map((r) => r.pr.number)).toEqual([2]);
    expect(prWaitingRows([row(asked), row(also), row(mine)], board, 'All projects').map((r) => r.pr.number)).toEqual([1, 2]);
  });

  it('has none while [queue] leaves reviews out', () => {
    const board = fx.board({ prs: [asked], queue: { order: ['needs', 'unread'], due_within: 0 } });
    expect(prWaitingRows([row(asked)], board, 'All projects')).toEqual([]);
    expect(prWaitingRows([row(asked)], null, 'All projects')).toEqual([]);
  });
});

describe('todoWaitingRows', () => {
  const due = fx.todo({ id: 'due', due: '2026-11-01' });
  const ready = fx.todo({ id: 'ready' });
  const hidden = fx.todo({ id: 'hidden' });
  const queue = { due: [waiting('due', NOW - 1000, 3)], ready: [waiting('ready', NOW - 50, 0), waiting('hidden', NOW - 60, 1)] };

  it('goes through the due todos the list shows, then the ready ones', () => {
    const rows = todoWaitingRows([listedTodo(ready), listedTodo(due)], queue, NOW);
    expect(rows.map((r) => [r.item.todo.id, r.waits])).toEqual([
      ['due', 'due'],
      ['ready', 'ready'],
    ]);
  });

  it('leaves out one this window marked done, archived, snoozed or assigned a session ahead of the board', () => {
    const list = [listedTodo({ ...due, status: 'done' }), listedTodo({ ...ready, snoozedUntil: NOW + 60_000 })];
    expect(todoWaitingRows(list, queue, NOW)).toEqual([]);
    expect(todoWaitingRows([listedTodo({ ...ready, archivedAt: NOW })], queue, NOW)).toEqual([]);
    expect(todoWaitingRows([listedTodo({ ...due, sessions: ['claude:a:1'] })], queue, NOW)).toEqual([]);
    expect(todoWaitingRows([listedTodo({ ...due, primary: { session: 'claude:a:1', since: NOW } })], queue, NOW)).toEqual([]);
    expect(todoWaitingRows([listedTodo({ ...ready, pending: [{ lane: '/a', harness: 'claude', account: 'a', at: NOW }] })], queue, NOW)).toEqual([]);
    // A snooze that's over is no snooze.
    expect(todoWaitingRows([listedTodo({ ...ready, snoozedUntil: NOW - 1 })], queue, NOW)).toHaveLength(1);
    expect(todoWaitingRows([listedTodo(ready)], undefined, NOW)).toEqual([]);
  });

  it('reads ready todos in your order, after the due ones', () => {
    const rows: TodoQueueRow[] = [
      { item: listedTodo(ready), waiting: waiting('ready', NOW - 50, 2), waits: 'ready' },
      { item: listedTodo(hidden), waiting: waiting('hidden', NOW - 10, 1), waits: 'ready' },
      { item: listedTodo(due), waiting: waiting('due', NOW - 5, 9), waits: 'due' },
    ];
    const reads = inOrder(TODO_READS, ['needs', 'reviews', 'due', 'unread', 'ready']);
    expect([...rows].sort((a, b) => byPlace(placeOf(a, reads), placeOf(b, reads))).map((r) => r.item.todo.id)).toEqual(['due', 'hidden', 'ready']);
  });
});

describe('elsewhereOffer', () => {
  const ORDER: readonly Waits[] = ['needs', 'reviews', 'due', 'unread', 'ready'];
  type Row = { key: string; waits: Waits; since: number; raised?: Raised; first?: boolean };
  const READS: Reads<Row> = { keyOf: (r) => r.key, sinceOf: (r) => r.since, waitsOf: (r) => r.waits, raisedOf: (r) => r.raised, finishedOf: () => false, firstOf: (r) => !!r.first, titleOf: (r) => r.key };
  const row = (key: string, waits: Waits, extra: Partial<Row> = {}): Row => ({ key, waits, since: NOW - 1000, ...extra });
  // Each view's rows not yet dealt with, in the queue's order, as useWaitingQueue.ts `pending` has them.
  const views = (rows: Partial<Record<'sessions' | 'prs' | 'todo', Row[]>>, order = ORDER) => {
    const reads = inOrder(READS, order);
    return (['sessions', 'prs', 'todo'] as const).map((v) => viewWaits(v, inQueueOrder(rows[v] ?? [], reads), reads));
  };

  it('offers the view whose first waiting comes first in your order, saying how many of its kind', () => {
    const w = views({ sessions: [row('n', 'needs'), row('u', 'unread')], todo: [row('d1', 'due'), row('d2', 'due'), row('r', 'ready')] });
    expect(elsewhereOffer('prs', w, 1)).toEqual({ view: 'sessions', words: 'Nothing else waits in PRs', title: '2 sessions in Sessions' });
    // With none of its own, nothing waits there at all.
    expect(elsewhereOffer('prs', w)?.words).toBe('Nothing waits in PRs');
    expect(elsewhereOffer('sessions', w, 3)).toEqual({ view: 'todo', words: 'Nothing else waits in Sessions', title: '2 due todos in Todo' });
    expect(elsewhereOffer('prs', views({ todo: [row('r', 'ready')] }))?.title).toBe('1 ready todo in Todo');
    expect(elsewhereOffer('sessions', views({ prs: [row('p', 'reviews')], todo: [row('d', 'due')] }))?.title).toBe('1 review in PRs');
    // Your order: finished turns ahead of reviews.
    expect(elsewhereOffer('todo', views({ sessions: [row('u', 'unread')], prs: [row('p', 'reviews')] }, ['unread', 'needs', 'reviews', 'due', 'ready']))?.view).toBe('sessions');
  });

  it('goes by what hasn’t been dealt with: a finished turn after a review, once what needed you was', () => {
    // The session that needed you was answered here, though the board still lists it: only the
    // finished turn is left of Sessions', and it comes after the review.
    expect(elsewhereOffer('todo', views({ sessions: [row('u', 'unread')], prs: [row('p', 'reviews')] }))?.view).toBe('prs');
  });

  it('offers a session moved to the top, then the reviews of a repo that puts them first, ahead of your order', () => {
    expect(elsewhereOffer('todo', views({ sessions: [row('n', 'needs')], prs: [row('p', 'reviews', { first: true })] }))?.view).toBe('prs');
    expect(elsewhereOffer('todo', views({ sessions: [row('u', 'unread', { raised: { at: NOW } })], prs: [row('p', 'reviews', { first: true })] }))?.view).toBe('sessions');
  });

  it('never offers the view’s own, Kanban’s being Todo’s', () => {
    expect(elsewhereOffer('todo', views({ todo: [row('d', 'due')] }))).toBeNull();
    expect(elsewhereOffer('kanban', views({ todo: [row('d', 'due')], prs: [row('p', 'reviews')] }))?.view).toBe('prs');
    expect(elsewhereOffer('sessions', views({}))).toBeNull();
  });
});
