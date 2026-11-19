import { describe, expect, it } from 'vitest';
import * as fx from './testing/fixtures';
import { byPlace, inOrder, placeOf } from './queue';
import { prRowOf } from './boardLists';
import { todoItem } from './todos';
import type { BarTodo, PrItem, Todo } from './types';
import { elsewhereOffer, prWaitingRows, TODO_READS, todoWaitingRows, type TodoQueueRow } from './viewQueue';

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

  it('leaves out one this window marked done, archived or snoozed ahead of the board', () => {
    const list = [listedTodo({ ...due, status: 'done' }), listedTodo({ ...ready, snoozedUntil: NOW + 60_000 })];
    expect(todoWaitingRows(list, queue, NOW)).toEqual([]);
    expect(todoWaitingRows([listedTodo({ ...ready, archivedAt: NOW })], queue, NOW)).toEqual([]);
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
  const ORDER = ['needs', 'reviews', 'due', 'unread', 'ready'] as const;

  it('offers the first kind in your order that waits in another view', () => {
    expect(elsewhereOffer('prs', { needs: 1, unread: 1, due: 2 }, ORDER, 1)).toEqual({ view: 'sessions', words: 'Nothing else waits in PRs', title: '2 sessions in Sessions' });
    // With none of its own, nothing waits there at all.
    expect(elsewhereOffer('prs', { needs: 1 }, ORDER)?.words).toBe('Nothing waits in PRs');
    expect(elsewhereOffer('sessions', { reviews: 1, due: 2 }, ORDER, 3)).toEqual({ view: 'prs', words: 'Nothing else waits in Sessions', title: '1 review in PRs' });
    expect(elsewhereOffer('sessions', { due: 2, ready: 4 }, ORDER)?.title).toBe('2 due todos in Todo');
    expect(elsewhereOffer('prs', { ready: 1 }, ORDER)?.title).toBe('1 ready todo in Todo');
    // Your order: finished turns ahead of reviews.
    expect(elsewhereOffer('todo', { unread: 1, reviews: 3 }, ['unread', 'needs', 'reviews', 'due', 'ready'])?.view).toBe('sessions');
  });

  it('never offers the view’s own, Kanban’s being Todo’s', () => {
    expect(elsewhereOffer('todo', { due: 1, ready: 2 }, ORDER)).toBeNull();
    expect(elsewhereOffer('kanban', { due: 1, ready: 2, reviews: 1 }, ORDER)?.view).toBe('prs');
    expect(elsewhereOffer('sessions', {}, ORDER)).toBeNull();
  });
});
