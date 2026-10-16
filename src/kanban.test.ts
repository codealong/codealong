import { describe, expect, it } from 'vitest';
import type { KanbanCard, KanbanColumn } from './boardLists';
import { applyKanbanDrop, insertionAt, kanbanDrop, kanbanStep, type KanbanActs } from './kanban';
import * as fx from './testing/fixtures';
import { todoItem, type TodoOp } from './todos';
import type { Todo } from './types';
import type { SessionItem } from './ui/SessionsView';

const todoCard = (over: Partial<Todo> = {}) => todoItem(fx.todo(over), new Map(), new Map(), new Map());
const sessionCard = (key = 'claude:claude:s1'): SessionItem => {
  const lane = fx.lane();
  return { kind: 'agent', key: `${lane.id}::${key}`, lane, session: fx.session({ key }) };
};
const move = (d: ReturnType<typeof kanbanDrop>) => d && { move: d.move, word: d.word ?? null };

describe('kanbanDrop', () => {
  it('moves a todo the way its new column says', () => {
    const t = todoCard({ status: 'todo' });
    expect(move(kanbanDrop(t, 'todo', 'progress', null, null))).toEqual({ move: { kind: 'start' }, word: 'Start session' });
    expect(move(kanbanDrop(t, 'todo', 'review', null, null))).toEqual({ move: { kind: 'review', on: true }, word: 'Ready for review' });
    expect(move(kanbanDrop(t, 'todo', 'done', null, null))).toEqual({ move: { kind: 'status', status: 'done' }, word: 'Done' });
    // Within To do it keeps your order: a plain move, no word.
    expect(move(kanbanDrop(t, 'todo', 'todo', null, 2))).toEqual({ move: { kind: 'order', index: 2 }, word: null });
    expect(kanbanDrop(t, 'todo', 'todo', null, null)).toBeNull();
    const done = todoCard({ status: 'done' });
    expect(move(kanbanDrop(done, 'done', 'progress', null, null))).toEqual({ move: { kind: 'status', status: 'in-progress' }, word: 'Reopen' });
    expect(move(kanbanDrop(done, 'done', 'todo', null, null))).toEqual({ move: { kind: 'status', status: 'todo' }, word: 'Reopen' });
    expect(kanbanDrop(done, 'done', 'done', null, null)).toBeNull();
  });

  it('takes a todo out of review only where a mark put it there, not its PR', () => {
    const marked = todoCard({ status: 'in-progress', review: { note: '', since: 1 } });
    expect(move(kanbanDrop(marked, 'review', 'progress', null, null))).toEqual({ move: { kind: 'review', on: false }, word: 'Back to in progress' });
    const byPr = todoCard({ status: 'in-progress' });
    expect(kanbanDrop(byPr, 'review', 'progress', null, null)).toBeNull();
    // Nor does a column that sorts itself take a reorder.
    expect(kanbanDrop(byPr, 'progress', 'progress', null, 0)).toBeNull();
  });

  it('marks a session done, or adds it to a todo it’s dropped on', () => {
    const s = sessionCard();
    expect(move(kanbanDrop(s, 'progress', 'done', null, null))).toEqual({ move: { kind: 'done' }, word: 'Mark as done' });
    const t = todoCard({ id: 'x', status: 'in-progress' });
    const onto = kanbanDrop(s, 'progress', 'progress', t, null);
    expect(onto).toMatchObject({ onto: 'todo:x', move: { kind: 'link', todo: 'x' }, word: 'Add to todo' });
    // Not onto a done one; anywhere else, nothing.
    expect(kanbanDrop(s, 'progress', 'done', todoCard({ status: 'done' }), null)?.move).toEqual({ kind: 'done' });
    expect(kanbanDrop(s, 'progress', 'review', null, null)).toBeNull();
    expect(kanbanDrop(s, 'done', 'done', null, null)).toBeNull();
  });
});

describe('applyKanbanDrop', () => {
  const acting = () => {
    const ops: TodoOp[] = [];
    const did: string[] = [];
    const said: { message: string; undo: TodoOp[] }[] = [];
    const acts: KanbanActs = {
      run: (op) => ops.push(op),
      start: (r) => did.push(`start ${r.todo.id}`),
      markDone: (r) => did.push(`done ${r.session.key}`),
      order: (id, index) => did.push(`order ${id} ${index}`),
      said: (message, undo) => said.push({ message, undo }),
    };
    return { ops, did, said, acts };
  };
  const drop = (card: KanbanCard, from: KanbanColumn['id'], to: KanbanColumn['id'], onto: KanbanCard | null = null, index: number | null = null) => {
    const d = kanbanDrop(card, from, to, onto, index);
    if (!d) throw new Error('no drop');
    return d;
  };

  it('changes a todo, with an Undo that puts back its status, wait and review', () => {
    const t = todoCard({ id: 'a', title: 'Ship it', status: 'in-progress', review: { note: 'x', since: 5 } });
    const { ops, said, acts } = acting();
    applyKanbanDrop(t, drop(t, 'review', 'done'), acts);
    expect(ops).toEqual([{ op: 'update', id: 'a', patch: { status: 'done' } }]);
    expect(said).toEqual([{ message: 'Marked “Ship it” done.', undo: [{ op: 'update', id: 'a', patch: { status: 'in-progress', waiting: null, review: { note: 'x', since: 5 } } }] }]);
  });

  it('reviews a done todo by reopening it, and sends a marked one back', () => {
    const { ops, acts } = acting();
    const done = todoCard({ id: 'd', status: 'done' });
    applyKanbanDrop(done, drop(done, 'done', 'review'), acts);
    const marked = todoCard({ id: 'm', status: 'in-progress', review: { note: '', since: 1 } });
    applyKanbanDrop(marked, drop(marked, 'review', 'progress'), acts);
    expect(ops).toEqual([
      { op: 'update', id: 'd', patch: { status: 'in-progress', review: { note: '', since: 0 } } },
      { op: 'update', id: 'm', patch: { review: null } },
    ]);
  });

  it('starts, reorders, marks a session done and adds it to a todo through the window', () => {
    const { ops, did, acts } = acting();
    const t = todoCard({ id: 't' });
    applyKanbanDrop(t, drop(t, 'todo', 'progress'), acts);
    applyKanbanDrop(t, drop(t, 'todo', 'todo', null, 3), acts);
    const s = sessionCard('claude:claude:k');
    applyKanbanDrop(s, drop(s, 'progress', 'done'), acts);
    applyKanbanDrop(s, drop(s, 'progress', 'progress', todoCard({ id: 'on', status: 'in-progress' })), acts);
    expect(did).toEqual(['start t', 'order t 3', 'done claude:claude:k']);
    expect(ops).toEqual([{ op: 'link', id: 'on', session: 'claude:claude:k', linked: true }]);
  });
});

describe('insertionAt', () => {
  it('lands before the first card whose middle is below the pointer', () => {
    const spans = [
      { top: 0, bottom: 40 },
      { top: 50, bottom: 90 },
    ];
    expect([insertionAt(5, spans), insertionAt(30, spans), insertionAt(80, spans), insertionAt(200, spans), insertionAt(5, [])]).toEqual([0, 1, 2, 2, 0]);
  });
});

describe('kanbanStep', () => {
  const card = (key: string): KanbanCard => ({ ...sessionCard(key), key });
  const columns: KanbanColumn[] = [
    { id: 'todo', label: 'To do', cards: [card('a'), card('b'), card('c')] },
    { id: 'progress', label: 'In progress', cards: [] },
    { id: 'review', label: 'In review', cards: [card('d')] },
    { id: 'done', label: 'Done', cards: [card('e')] },
  ];
  it('goes up and down a column, and across to the next column that has cards', () => {
    expect(kanbanStep(columns, 'b', 'down', false)).toBe('c');
    expect(kanbanStep(columns, 'c', 'down', false)).toBe('c');
    expect(kanbanStep(columns, 'a', 'up', false)).toBe('a');
    // In progress has none: across it, to as far down as the next column goes.
    expect(kanbanStep(columns, 'c', 'right', false)).toBe('d');
    expect(kanbanStep(columns, 'd', 'left', false)).toBe('a');
    // Done only while it's open.
    expect(kanbanStep(columns, 'd', 'right', false)).toBe('d');
    expect(kanbanStep(columns, 'd', 'right', true)).toBe('e');
    expect(kanbanStep(columns, null, 'down', false)).toBe('a');
  });
});
