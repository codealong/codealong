import { describe, expect, it } from 'vitest';
import type { KanbanCard, KanbanColumn } from './boardLists';
import { STAGE_COLUMNS, stageOfColumn, type BoardColumn } from './boards/model';
import { sessionSubject, todoSubject, type Subject } from './boards/rules';
import { applyKanbanDrop, boardOrderWith, columnWith, insertionAt, inSlots, kanbanDrop, kanbanStep, type DropContext, type KanbanActs } from './kanban';
import * as fx from './testing/fixtures';
import { NOW } from './testing/fixtures';
import { todoItem, type TodoOp } from './todos';
import type { Lane, Todo } from './types';
import type { SessionItem } from './ui/SessionsView';

const todoCard = (over: Partial<Todo> = {}, lanes: Lane[] = []) => todoItem(fx.todo(over), new Map(lanes.map((l) => [l.id, l])), new Map(), new Map());
const sessionCard = (key = 'claude:claude:s1'): SessionItem => {
  const lane = fx.lane();
  return { kind: 'agent', key: `${lane.id}::${key}`, lane, session: fx.session({ key }) };
};

/** A board's columns as the view shows them, with no cards. */
const shown = (defs: BoardColumn[]): KanbanColumn[] => defs.map((c) => ({ id: c.id, label: c.name, column: c, stage: stageOfColumn(c), cards: [] }));
const ctxOf = (defs: BoardColumn[], cards: KanbanCard[] = [], placed?: Record<string, string>): DropContext => {
  const subjects = new Map<string, Subject>(cards.map((c) => (c.kind === 'todo' ? todoSubject(c, []) : sessionSubject(c, [], []))).map((s) => [s.card, s]));
  return { columns: defs, placed, subjects, todos: [], now: NOW };
};
const stages = shown(STAGE_COLUMNS);
const col = (id: string, cols = stages) => cols.find((c) => c.id === id) as KanbanColumn;
const drop = (card: KanbanCard, from: string, to: string, onto: KanbanCard | null = null, index: number | null = null, cols = stages, ctx = ctxOf(STAGE_COLUMNS, [card])) =>
  kanbanDrop(card, col(from, cols), col(to, cols), onto, index, ctx);
const said = (d: ReturnType<typeof kanbanDrop>) => d && { moves: d.moves, word: d.word ?? null };

describe('kanbanDrop on the stage columns', () => {
  it('moves a todo the way its new column says', () => {
    const t = todoCard({ status: 'todo' });
    expect(said(drop(t, 'todo', 'progress'))).toEqual({ moves: [{ kind: 'start' }], word: 'Start session' });
    expect(said(drop(t, 'todo', 'review'))).toEqual({ moves: [{ kind: 'review', on: true }], word: 'Ready for review' });
    expect(said(drop(t, 'todo', 'done'))).toEqual({ moves: [{ kind: 'status', status: 'done' }], word: 'Done' });
    // Within To do it keeps your order: a plain move, no word.
    expect(said(drop(t, 'todo', 'todo', null, 2))).toEqual({ moves: [{ kind: 'order', index: 2 }], word: null });
    expect(drop(t, 'todo', 'todo')).toBeNull();
    const done = todoCard({ status: 'done' });
    expect(said(drop(done, 'done', 'progress'))).toEqual({ moves: [{ kind: 'status', status: 'in-progress' }], word: 'Reopen' });
    expect(said(drop(done, 'done', 'todo'))).toEqual({ moves: [{ kind: 'status', status: 'todo' }], word: 'Reopen' });
    expect(drop(done, 'done', 'done')).toBeNull();
  });

  it('takes a todo out of review only where a mark put it there, not its PR', () => {
    const marked = todoCard({ status: 'in-progress', review: { note: '', since: 1 } });
    expect(said(drop(marked, 'review', 'progress'))).toEqual({ moves: [{ kind: 'review', on: false }], word: 'Back to in progress' });
    const open = fx.lane({ id: '/r/open', pr: fx.pr({ state: 'open' }) });
    const byPr = todoCard({ status: 'in-progress', worktrees: ['/r/open'] }, [open]);
    expect(drop(byPr, 'review', 'progress')).toBeNull();
    // Back to To do it can go: a todo not started has no PR in review.
    expect(said(drop(byPr, 'review', 'todo'))?.word).toBe('Back to to do');
  });

  it('marks a session done, or adds it to a todo it’s dropped on', () => {
    const s = sessionCard();
    expect(said(drop(s, 'progress', 'done'))).toEqual({ moves: [{ kind: 'done' }], word: 'Mark as done' });
    const t = todoCard({ id: 'x', status: 'in-progress' });
    expect(drop(s, 'progress', 'progress', t)).toMatchObject({ onto: 'todo:x', moves: [{ kind: 'link', todo: 'x' }], word: 'Add to todo' });
    // Not onto a done one; anywhere else, nothing.
    expect(drop(s, 'progress', 'done', todoCard({ status: 'done' }))?.moves).toEqual([{ kind: 'done' }]);
    expect(drop(s, 'progress', 'review')).toBeNull();
    expect(drop(s, 'done', 'done')).toBeNull();
  });
});

describe('kanbanDrop on columns of your own', () => {
  const next: BoardColumn = { id: 'next', name: 'Next', sort: 'hand' };
  const later: BoardColumn = { id: 'later', name: 'Later', sort: 'hand' };
  const defs = [next, later, ...STAGE_COLUMNS.filter((c) => c.id !== 'todo')];
  const cols = shown(defs);

  it('puts a todo not started in a column of yours, at the place it’s dropped', () => {
    const t = todoCard({ id: 'a' });
    expect(said(drop(t, 'next', 'later', null, null, cols, ctxOf(defs, [t])))).toEqual({ moves: [{ kind: 'place', column: 'later' }], word: 'Move to Later' });
    expect(drop(t, 'next', 'later', null, 1, cols, ctxOf(defs, [t]))?.moves).toEqual([{ kind: 'place', column: 'later' }, { kind: 'order', index: 1 }]);
  });

  it('sends work under way or done back to To do first, as the stage column holds it', () => {
    const doing = todoCard({ id: 'b', status: 'in-progress' });
    expect(said(drop(doing, 'progress', 'next', null, null, cols, ctxOf(defs, [doing])))).toEqual({ moves: [{ kind: 'status', status: 'todo' }, { kind: 'place', column: 'next' }], word: 'Move to Next' });
    const done = todoCard({ id: 'c', status: 'done' });
    expect(drop(done, 'done', 'later', null, null, cols, ctxOf(defs, [done]))?.moves).toEqual([{ kind: 'status', status: 'todo' }, { kind: 'place', column: 'later' }]);
  });

  it('puts a session in one only where no rule column holds it', () => {
    const s = sessionCard('claude:claude:k');
    expect(drop(s, 'progress', 'next', null, null, cols, ctxOf(defs, [s]))).toBeNull();
    const open = shown([next]);
    expect(kanbanDrop(s, { ...open[0], id: 'x' }, open[0], null, null, ctxOf([next], [s]))?.moves).toEqual([{ kind: 'place', column: 'next' }]);
  });
});

describe('kanbanDrop on label columns', () => {
  const bug: BoardColumn = { id: 'bug', name: 'Bugs', rule: { labels: ['l-bug'] }, sort: 'move' };
  const ui: BoardColumn = { id: 'ui', name: 'UI', rule: { labels: ['l-ui'] }, sort: 'move' };
  const other: BoardColumn = { id: 'other', name: 'Other', sort: 'hand' };
  const defs = [bug, ui, other];
  const cols = shown(defs);

  it('gives a todo the column’s label, and takes away the label of the one it leaves', () => {
    const t = todoCard({ id: 'a', labels: ['l-bug'] });
    expect(said(drop(t, 'bug', 'ui', null, null, cols, ctxOf(defs, [t])))).toEqual({ moves: [{ kind: 'labels', add: ['l-ui'], remove: ['l-bug'] }], word: 'Move to UI' });
    expect(drop(t, 'bug', 'other', null, null, cols, ctxOf(defs, [t]))?.moves).toEqual([{ kind: 'labels', add: [], remove: ['l-bug'] }, { kind: 'place', column: 'other' }]);
    const plain = todoCard({ id: 'b' });
    expect(said(drop(plain, 'other', 'bug', null, null, cols, ctxOf(defs, [plain])))).toEqual({ moves: [{ kind: 'labels', add: ['l-bug'], remove: [] }], word: 'Add to Bugs' });
  });

  it('gives a session no label', () => {
    const s = sessionCard();
    expect(drop(s, 'other', 'bug', null, null, cols, ctxOf(defs, [s]))).toBeNull();
  });
});

describe('applyKanbanDrop', () => {
  const acting = () => {
    const ops: TodoOp[] = [];
    const did: string[] = [];
    const told: { message: string; undo: () => void }[] = [];
    const acts: KanbanActs = {
      run: (op) => ops.push(op),
      start: (r) => did.push(`start ${r.todo.id}`),
      markDone: (r) => did.push(`done ${r.session.key}`),
      order: (card, column, index) => did.push(`order ${card.key} ${column} ${index}`),
      place: (card, column) => did.push(`place ${card.key} ${column}`),
      said: (message, undo) => told.push({ message, undo }),
    };
    return { ops, did, told, acts };
  };
  const must = (d: ReturnType<typeof kanbanDrop>) => {
    if (!d) throw new Error('no drop');
    return d;
  };

  it('changes a todo, with an Undo that puts back its status, wait, review and labels', () => {
    const t = todoCard({ id: 'a', title: 'Ship it', status: 'in-progress', review: { note: 'x', since: 5 } });
    const { ops, told, acts } = acting();
    applyKanbanDrop(t, must(drop(t, 'review', 'done')), acts, null);
    expect(ops).toEqual([{ op: 'update', id: 'a', patch: { status: 'done' } }]);
    expect(told.map((x) => x.message)).toEqual(['Marked “Ship it” done.']);
    told[0].undo();
    expect(ops[1]).toEqual({ op: 'update', id: 'a', patch: { status: 'in-progress', waiting: null, review: { note: 'x', since: 5 }, labels: [] } });
  });

  it('reviews a done todo by reopening it, and sends a marked one back', () => {
    const { ops, acts } = acting();
    const done = todoCard({ id: 'd', status: 'done' });
    applyKanbanDrop(done, must(drop(done, 'done', 'review')), acts, null);
    const marked = todoCard({ id: 'm', status: 'in-progress', review: { note: '', since: 1 } });
    applyKanbanDrop(marked, must(drop(marked, 'review', 'progress')), acts, null);
    expect(ops).toEqual([
      { op: 'update', id: 'd', patch: { review: { note: '', since: 0 }, status: 'in-progress' } },
      { op: 'update', id: 'm', patch: { review: null } },
    ]);
  });

  it('starts, reorders, marks a session done and adds it to a todo through the window', () => {
    const { ops, did, acts } = acting();
    const t = todoCard({ id: 't' });
    applyKanbanDrop(t, must(drop(t, 'todo', 'progress')), acts, null);
    applyKanbanDrop(t, must(drop(t, 'todo', 'todo', null, 3)), acts, null);
    const s = sessionCard('claude:claude:k');
    applyKanbanDrop(s, must(drop(s, 'progress', 'done')), acts, null);
    applyKanbanDrop(s, must(drop(s, 'progress', 'progress', todoCard({ id: 'on', status: 'in-progress' }))), acts, null);
    expect(did).toEqual(['start t', 'order todo:t todo 3', `done claude:claude:k`]);
    expect(ops).toEqual([{ op: 'link', id: 'on', session: 'claude:claude:k', linked: true }]);
  });

  it('puts a card in a column of yours, and its Undo puts it back where it was', () => {
    const next: BoardColumn = { id: 'next', name: 'Next', sort: 'hand' };
    const defs = [next, ...STAGE_COLUMNS.filter((c) => c.id !== 'todo')];
    const doing = todoCard({ id: 'b', title: 'Doing', status: 'in-progress' });
    const { ops, did, told, acts } = acting();
    applyKanbanDrop(doing, must(drop(doing, 'progress', 'next', null, null, shown(defs), ctxOf(defs, [doing]))), acts, 'later');
    expect(ops).toEqual([{ op: 'update', id: 'b', patch: { status: 'todo' } }]);
    expect(did).toEqual(['place todo:b next']);
    expect(told[0].message).toBe('Moved “Doing” back to To do.');
    told[0].undo();
    expect(did).toEqual(['place todo:b next', 'place todo:b later']);
  });
});

describe('keeping your order', () => {
  it('puts a card at an insertion point among a column’s cards, from in it or out of it', () => {
    expect(columnWith(['a', 'b', 'c'], 'a', 2)).toEqual(['b', 'a', 'c']);
    expect(columnWith(['a', 'b', 'c'], 'c', 0)).toEqual(['c', 'a', 'b']);
    expect(columnWith(['a', 'b'], 'x', 1)).toEqual(['a', 'x', 'b']);
    expect(columnWith([], 'x', 5)).toEqual(['x']);
  });

  it('puts a column’s order back in the order of everything, the rest where they were', () => {
    expect(inSlots(['t1', 'o1', 't2', 't3'], ['t3', 't1'])).toEqual(['t3', 'o1', 't2', 't1']);
    expect(inSlots(['t1', 't2'], ['gone', 't2', 't1'])).toEqual(['t2', 't1']);
    expect(boardOrderWith(['x', 'a', 'y'], ['a', 'b'])).toEqual(['a', 'b', 'x', 'y']);
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
  const columns: KanbanColumn[] = shown(STAGE_COLUMNS).map((c, i) => ({ ...c, cards: [[card('a'), card('b'), card('c')], [], [card('d')], [card('e')]][i] }));
  const open = (opened: boolean) => (c: KanbanColumn) => !c.column?.folded || opened;
  it('goes up and down a column, and across to the next column that has cards', () => {
    expect(kanbanStep(columns, 'b', 'down', open(false))).toBe('c');
    expect(kanbanStep(columns, 'c', 'down', open(false))).toBe('c');
    expect(kanbanStep(columns, 'a', 'up', open(false))).toBe('a');
    // In progress has none: across it, to as far down as the next column goes.
    expect(kanbanStep(columns, 'c', 'right', open(false))).toBe('d');
    expect(kanbanStep(columns, 'd', 'left', open(false))).toBe('a');
    // Done, which starts folded, only while it's open.
    expect(kanbanStep(columns, 'd', 'right', open(false))).toBe('d');
    expect(kanbanStep(columns, 'd', 'right', open(true))).toBe('e');
    expect(kanbanStep(columns, null, 'down', open(false))).toBe('a');
  });
});

