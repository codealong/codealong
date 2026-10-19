import { describe, expect, it } from 'vitest';
import { sessionItemsOf } from '../boardLists';
import { sessionHomes } from '../lanes';
import * as fx from '../testing/fixtures';
import { NOW } from '../testing/fixtures';
import { todoItem } from '../todos';
import type { Lane, Todo } from '../types';
import type { BoardColumn, BoardDoc } from './model';
import { columnOf, matches, memberTest, subjectsOf, type Subject } from './rules';

const DAY = 86_400_000;

/** What the rules ask of a project of one worktree with a working session on a labelled todo, a
 *  finished one on nothing, and an open PR. */
function project() {
  const working = fx.session({ key: 'claude:claude:w', account: 'claude', status: 'working', title: 'Fix the login page' });
  const done = fx.session({ key: 'codex:codex:d', account: 'codex', status: 'idle', lastActivity: NOW - 10 * DAY, title: 'Old work' });
  const lane: Lane = fx.lane({ id: '/r/app', repoSlug: 'acme/app', sessions: [working, done] });
  const todos: Todo[] = [fx.todo({ id: 'rel', labels: ['l-rel'], status: 'in-progress', sessions: [working.key], title: 'Release 0.3' }), fx.todo({ id: 'plain', title: 'Tidy up' })];
  const lanes = [lane];
  const todoItems = todos.map((t) => todoItem(t, new Map([[lane.id, lane]]), sessionHomes(lanes), new Map()));
  const prs = [fx.prItem({ key: 'acme/app#7', repoSlug: 'acme/app', state: 'open', sessions: [{ lane: '/r/app', key: done.key }], updatedAt: NOW - DAY })];
  return subjectsOf({ todoItems, todos, sessionItems: sessionItemsOf(lanes), lanes, prs });
}

const at = (subjects: Map<string, Subject>, card: string) => subjects.get(card) as Subject;

describe('what a thing on a board is to its rules', () => {
  const s = project();

  it('gives a session its todos’ labels, and a todo its sessions’ statuses and accounts', () => {
    expect(at(s, 'session:claude:claude:w')).toMatchObject({ ref: 'session', labels: ['l-rel'], statuses: ['working'], accounts: ['claude'], repos: ['acme/app'], stage: 'progress' });
    expect(at(s, 'todo:rel')).toMatchObject({ labels: ['l-rel'], statuses: ['working'], accounts: ['claude'], stage: 'progress' });
    expect(at(s, 'todo:plain')).toMatchObject({ statuses: [], stage: 'todo' });
    expect(at(s, 'worktree:/r/app')).toMatchObject({ labels: ['l-rel'], accounts: ['claude', 'codex'] });
    expect(at(s, 'pr:acme/app#7')).toMatchObject({ stage: 'review', accounts: ['codex'], repos: ['acme/app'] });
  });

  it('takes in a thing when every part of a rule holds, and any pick within one', () => {
    const w = at(s, 'session:claude:claude:w');
    expect(matches({}, w, NOW)).toBe(true);
    expect(matches({ labels: ['l-rel', 'l-other'], kinds: ['session'] }, w, NOW)).toBe(true);
    expect(matches({ labels: ['l-rel'], kinds: ['todo'] }, w, NOW)).toBe(false);
    expect(matches({ repos: ['ACME/App'] }, w, NOW)).toBe(true);
    expect(matches({ text: 'login fix' }, w, NOW)).toBe(true);
    expect(matches({ text: 'logout' }, w, NOW)).toBe(false);
    expect(matches({ active: '7d' }, at(s, 'session:codex:codex:d'), NOW)).toBe(false);
    expect(matches({ active: '30d' }, at(s, 'session:codex:codex:d'), NOW)).toBe(true);
    expect(matches({ statuses: ['needs'] }, w, NOW)).toBe(false);
  });

  it('holds what its rule takes in and what was added, less what was taken off', () => {
    const board: BoardDoc = { id: 'b-1', name: 'Release', project: 'Acme', rule: { labels: ['l-rel'] }, added: ['pr:acme/app#7'], removed: ['worktree:/r/app'] };
    const on = memberTest(board, s, NOW);
    expect([...s.keys()].filter(on).sort()).toEqual(['pr:acme/app#7', 'session:claude:claude:w', 'todo:rel']);
    // Added by hand, it's on the board though nothing of the project's has it now.
    expect(on('todo:gone')).toBe(false);
    expect(memberTest({ ...board, added: ['todo:gone'] }, s, NOW)('todo:gone')).toBe(true);
    expect(memberTest({ ...board, rule: null, added: [] }, s, NOW)('todo:rel')).toBe(false);
    expect(memberTest({ ...board, rule: {}, removed: [] }, s, NOW)('anything')).toBe(true);
  });
});

describe('columnOf', () => {
  const s = project();
  const cols: BoardColumn[] = [
    { id: 'rel', name: 'Release', rule: { labels: ['l-rel'] }, sort: 'move' },
    { id: 'inbox', name: 'Inbox', sort: 'hand' },
    { id: 'later', name: 'Later', sort: 'hand' },
  ];

  it('goes by the first column whose rule takes it, else where it was put, else the first without a rule', () => {
    expect(columnOf(at(s, 'todo:rel'), cols, { 'todo:rel': 'later' }, NOW)).toBe('rel');
    expect(columnOf(at(s, 'todo:plain'), cols, { 'todo:plain': 'later' }, NOW)).toBe('later');
    expect(columnOf(at(s, 'todo:plain'), cols, undefined, NOW)).toBe('inbox');
    // A place in a column that has a rule, or that's gone, isn't a place.
    expect(columnOf(at(s, 'todo:plain'), cols, { 'todo:plain': 'rel' }, NOW)).toBe('inbox');
    expect(columnOf(at(s, 'todo:plain'), [cols[0]], undefined, NOW)).toBeNull();
  });
});
