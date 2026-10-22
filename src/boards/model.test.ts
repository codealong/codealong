import { describe, expect, it } from 'vitest';
import type { Label } from '../types';
import { applyOp, boardChoices, boardName, boardsFor, boardsIn, followsMove, cleanRule, columnsOf, newColumnId, presetColumns, ruleWords, STAGE_COLUMNS, stageOfColumn, takesAll, type BoardDoc } from './model';

const NOW = 1_790_000_000_000;
const named = (id: string, name: string, over: Partial<BoardDoc> = {}): BoardDoc => ({ id, name, project: 'Acme', rule: null, added: [], removed: [], createdAt: 5, ...over });
const labels: Label[] = [
  { id: 'l-1', name: 'release', color: 'blue' },
  { id: 'l-2', name: 'bug', color: 'red' },
];

describe('a project’s boards', () => {
  it('lists only the boards made in the project, as they were made: none until one is', () => {
    const list = boardsIn([named('b-2', 'Later', { createdAt: 9 }), named('b-1', 'Release'), named('b-3', 'Elsewhere', { project: 'Atlas' })], 'Acme');
    expect(list.map((b) => b.id)).toEqual(['b-1', 'b-2']);
    expect(boardsIn(list, 'Kestrel')).toEqual([]);
    expect(boardsIn([], 'All projects')).toEqual([]);
  });

  it('lists every project’s boards under All projects, after its own', () => {
    const all = 'All projects';
    const boards = [named('b-3', 'Later'), named('b-5', 'Atlas', { project: 'Atlas', createdAt: 0 }), named('b-2', 'Ideas', { project: all }), named('b-1', 'Release', { createdAt: 1 }), named('b-4', 'Site', { project: 'Atlas' })];
    expect(boardsIn(boards, all).map((b) => b.id)).toEqual(['b-2', 'b-1', 'b-3', 'b-5', 'b-4']);
    expect(boardsIn(boards, 'Acme').map((b) => b.id)).toEqual(['b-1', 'b-3']);
  });

  it('offers a thing the boards that would show it, and names another project’s board with its project', () => {
    const boards = [named('b-0', 'Everything', { project: 'All projects', rule: {} }), named('b-2', 'Ideas', { project: 'All projects' }), named('b-1', 'Release'), named('b-4', 'Site', { project: 'Atlas' })];
    expect(boardsFor(boards, 'Atlas').map((b) => b.id)).toEqual(['b-0', 'b-2', 'b-4']);
    expect(boardsFor(boards, '').map((b) => b.id)).toEqual(['b-0', 'b-2']);
    expect(boardsFor(boards, null)).toBe(boards);
    expect(boardName(boards[3], 'All projects')).toBe('Site · Atlas');
    expect(boardName(boards[3], 'Atlas')).toBe('Site');
  });

  it('offers things picked together each board that would show any of them, with the ones it would show', () => {
    const boards = [named('b-2', 'Ideas', { project: 'All projects' }), named('b-1', 'Release', { added: ['session:a'] }), named('b-4', 'Site', { project: 'Atlas', added: ['session:b'] }), named('b-5', 'Kestrel', { project: 'Kestrel' })];
    const cards = [
      { card: 'session:a', project: 'Acme' },
      { card: 'session:b', project: 'Atlas' },
      { card: 'session:c', project: 'Acme' },
    ];
    // Each board is asked once, however many there are.
    const asked: string[] = [];
    const holds = (b: BoardDoc) => {
      asked.push(b.id);
      return (card: string) => b.added.includes(card);
    };
    expect(boardChoices(boards, cards, holds).map((c) => [c.board.id, c.cards, c.fresh, c.on])).toEqual([
      ['b-2', ['session:a', 'session:b', 'session:c'], ['session:a', 'session:b', 'session:c'], false],
      ['b-1', ['session:a', 'session:c'], ['session:c'], false],
      // It holds every one it would show, so a pick takes it off.
      ['b-4', ['session:b'], [], true],
    ]);
    expect(asked).toEqual(['b-2', 'b-1', 'b-4']);
    // One whose project isn't known goes on any.
    expect(boardChoices(boards, [{ card: 'pr:x', project: null }], holds).map((c) => c.board.id)).toEqual(['b-2', 'b-1', 'b-4', 'b-5']);
  });

  it('has the four stage columns until it has its own', () => {
    expect(columnsOf(named('b-1', 'Release'))).toBe(STAGE_COLUMNS);
    expect(STAGE_COLUMNS.map(stageOfColumn)).toEqual(['todo', 'progress', 'review', 'done']);
    expect(stageOfColumn({ id: 'x', name: 'X', rule: { stages: ['review'], labels: ['l-1'] }, sort: 'hand' })).toBeNull();
    expect(stageOfColumn({ id: 'x', name: 'X', sort: 'hand' })).toBeNull();
  });
});

describe('applyOp', () => {
  it('makes a change as the engine makes it, and none to a board that isn’t there', () => {
    const one = applyOp([named('b-1', 'Release')], { op: 'add', id: 'b-1', cards: ['todo:t-1'] }, NOW);
    expect(one).toEqual([{ ...named('b-1', 'Release'), added: ['todo:t-1'], addedAt: { 'todo:t-1': NOW } }]);
    // A project's own board isn't made by a change to it any more.
    expect(applyOp(one, { op: 'add', id: 'p:Acme', cards: ['todo:t-2'] }, NOW)).toBe(one);
    expect(applyOp(one, { op: 'add', id: 'b-gone', cards: ['todo:t-2'] }, NOW)).toBe(one);
    // Its last board deleted, the project has none.
    expect(boardsIn(applyOp(one, { op: 'delete', id: 'b-1' }, NOW), 'Acme')).toEqual([]);
  });

  it('moves a board to another project with what it has', () => {
    const boards = [named('b-1', 'Release', { added: ['todo:t-1'] }), named('b-2', 'Later')];
    const moved = applyOp(boards, { op: 'project', id: 'b-1', project: 'Atlas' }, NOW);
    expect(moved[0]).toEqual({ ...boards[0], project: 'Atlas' });
    // It leaves the project's list for the other's.
    expect(boardsIn(moved, 'Acme').map((b) => b.id)).toEqual(['b-2']);
    expect(boardsIn(moved, 'Atlas').map((b) => b.id)).toEqual(['b-1']);
  });

  it('follows a board it moved only from the board itself, and only if the window is still there', () => {
    const at = { project: 'Acme', showing: true, board: 'b-1' };
    expect(followsMove('b-1', at, at)).toBe(true);
    // Moved from its sidebar menu while Todo shows, or another board, the window stays.
    expect(followsMove('b-1', { ...at, showing: false }, { ...at, showing: false })).toBe(false);
    expect(followsMove('b-2', at, at)).toBe(false);
    // All projects lists it still.
    expect(followsMove('b-1', { ...at, project: 'All projects' }, { ...at, project: 'All projects' })).toBe(false);
    // Gone elsewhere before the move came back, it isn't taken back.
    expect(followsMove('b-1', at, { ...at, showing: false })).toBe(false);
    expect(followsMove('b-1', at, { ...at, project: 'Atlas' })).toBe(false);
    expect(followsMove('b-1', at, { ...at, board: 'b-2' })).toBe(false);
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
