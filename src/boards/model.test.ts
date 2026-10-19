import { describe, expect, it } from 'vitest';
import type { Label } from '../types';
import { applyOp, boardsIn, cleanRule, columnsOf, newColumnId, presetColumns, projectBoard, ruleWords, STAGE_COLUMNS, stageOfColumn, takesAll, type BoardDoc } from './model';

const NOW = 1_790_000_000_000;
const named = (id: string, name: string, over: Partial<BoardDoc> = {}): BoardDoc => ({ id, name, project: 'Acme', rule: null, added: [], removed: [], createdAt: 5, ...over });
const labels: Label[] = [
  { id: 'l-1', name: 'release', color: 'blue' },
  { id: 'l-2', name: 'bug', color: 'red' },
];

describe('a project’s boards', () => {
  it('lists the project’s own first, whether or not it was ever written, then the rest as they were made', () => {
    const list = boardsIn([named('b-2', 'Later', { createdAt: 9 }), named('b-1', 'Release'), named('b-3', 'Elsewhere', { project: 'Atlas' })], 'Acme');
    expect(list.map((b) => b.id)).toEqual(['p:Acme', 'b-1', 'b-2']);
    expect(list[0]).toEqual(projectBoard('Acme'));
    expect(takesAll(list[0].rule)).toBe(true);
  });

  it('has the four stage columns until it has its own', () => {
    expect(columnsOf(projectBoard('Acme'))).toBe(STAGE_COLUMNS);
    expect(STAGE_COLUMNS.map(stageOfColumn)).toEqual(['todo', 'progress', 'review', 'done']);
    expect(stageOfColumn({ id: 'x', name: 'X', rule: { stages: ['review'], labels: ['l-1'] }, sort: 'hand' })).toBeNull();
    expect(stageOfColumn({ id: 'x', name: 'X', sort: 'hand' })).toBeNull();
  });
});

describe('applyOp', () => {
  it('makes a change as the engine makes it: a project’s own board joins the list as it first changes', () => {
    const one = applyOp([], { op: 'add', id: 'p:Acme', cards: ['todo:t-1'] }, NOW);
    expect(one).toEqual([{ ...projectBoard('Acme'), added: ['todo:t-1'], addedAt: { 'todo:t-1': NOW } }]);
    expect(applyOp(one, { op: 'delete', id: 'p:Acme' }, NOW)).toEqual(one);
    expect(applyOp(one, { op: 'add', id: 'b-gone', cards: ['todo:t-2'] }, NOW)).toBe(one);
  });

  it('puts back what’s added, and keeps off what’s taken off, out of its column and order too', () => {
    let boards = [named('b-1', 'Release')];
    boards = applyOp(boards, { op: 'add', id: 'b-1', cards: ['session:s1'] }, NOW);
    boards = applyOp(boards, { op: 'place', id: 'b-1', card: 'session:s1', column: 'next' }, NOW);
    boards = applyOp(boards, { op: 'order', id: 'b-1', order: ['session:s1', 'todo:t', 'session:s1'] }, NOW);
    expect(boards[0]).toMatchObject({ placed: { 'session:s1': 'next' }, order: ['session:s1', 'todo:t'] });
    boards = applyOp(boards, { op: 'remove', id: 'b-1', cards: ['session:s1'] }, NOW);
    expect(boards[0]).toMatchObject({ added: [], removed: ['session:s1'], placed: {}, addedAt: {}, order: ['todo:t'] });
    boards = applyOp(boards, { op: 'add', id: 'b-1', cards: ['session:s1'] }, NOW);
    expect(boards[0]).toMatchObject({ added: ['session:s1'], removed: [] });
  });

  it('makes, renames and deletes a board, and sends what was put in a column that’s gone back to the rules', () => {
    let boards = applyOp([], { op: 'create', board: named('b-1', '  Bugs   to fix ', { added: ['session:s1'] }) }, NOW);
    // What it's made with was added as it was made.
    expect(boards[0]).toMatchObject({ name: 'Bugs to fix', createdAt: NOW, addedAt: { 'session:s1': NOW } });
    boards = applyOp(boards, { op: 'rename', id: 'b-1', name: ' ' }, NOW);
    expect(boards[0].name).toBe('Bugs to fix');
    boards = applyOp(boards, { op: 'columns', id: 'b-1', columns: [{ id: 'a', name: 'A', sort: 'hand' }] }, NOW);
    boards = applyOp(boards, { op: 'place', id: 'b-1', card: 'todo:t', column: 'a' }, NOW);
    boards = applyOp(boards, { op: 'columns', id: 'b-1', columns: null }, NOW);
    expect(boards[0]).toMatchObject({ columns: null, placed: {} });
    expect(applyOp(boards, { op: 'delete', id: 'b-1' }, NOW)).toEqual([]);
  });
});

describe('rules in words', () => {
  it('says what a board takes in', () => {
    expect(ruleWords(null, labels)).toBe('Only what’s added');
    expect(ruleWords({}, labels)).toBe('Everything');
    expect(ruleWords({ kinds: ['todo', 'pr'], labels: ['l-1', 'l-gone'], stages: ['review'], statuses: ['needs'], repos: ['acme/app'], accounts: ['codex'], active: '7d', text: ' login ' }, labels)).toBe(
      'Todos or PRs, labelled release or a deleted label, in review, needs you, in acme/app, on codex, active in the last 7d, titled “login”',
    );
  });

  it('keeps only the parts a rule has', () => {
    expect(cleanRule({ kinds: [], labels: ['l-1'], text: '  x ', active: undefined })).toEqual({ labels: ['l-1'], text: 'x' });
    expect(takesAll(cleanRule({ kinds: [], text: '' }))).toBe(true);
    expect(takesAll(null)).toBe(false);
  });
});

describe('columns a board starts with', () => {
  it('are the stages, one per label and Other, or Inbox', () => {
    expect(presetColumns('stages', labels)).toBeNull();
    expect(presetColumns('blank', labels)).toEqual([{ id: 'inbox', name: 'Inbox', sort: 'hand' }]);
    expect(presetColumns('labels', labels)?.map((c) => [c.id, c.name, c.rule ?? null])).toEqual([
      ['label-l-1', 'release', { labels: ['l-1'] }],
      ['label-l-2', 'bug', { labels: ['l-2'] }],
      ['other', 'Other', null],
    ]);
    expect(newColumnId([{ id: 'c2', name: 'x', sort: 'hand' }, { id: 'c3', name: 'y', sort: 'hand' }])).toBe('c4');
  });
});
