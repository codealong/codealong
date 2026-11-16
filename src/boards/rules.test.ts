import { describe, expect, it } from 'vitest';
import { sessionItemsOf } from '../boardLists';
import { sessionHomes } from '../lanes';
import * as fx from '../testing/fixtures';
import { NOW } from '../testing/fixtures';
import { todoItem } from '../todos';
import type { AutoReview, Label, Lane, PrItem, Todo } from '../types';
import type { BoardColumn, BoardDoc } from './model';
import { columnOf, matches, memberTest, ownLabels, prStage, subjectsOf, todoStage, type Subject } from './rules';

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

describe('where an open PR is in its loop of checks, reviews and fixes', () => {
  const review = (over: Partial<AutoReview>): AutoReview => ({ stage: 'reviewing', head: 'c2', at: NOW, rounds: 1, ...over });
  const at = (over: Partial<PrItem>) => prStage(fx.prItem({ headOid: 'c2', ...over }));

  it('has a draft under way and a merged or closed one done', () => {
    expect(at({ state: 'draft', checks: fx.checks({ pending: 2 }) })).toBe('progress');
    expect(at({ state: 'merged' })).toBe('done');
    expect(at({ state: 'closed' })).toBe('done');
    expect(prStage(null)).toBeNull();
  });

  it('is Validating while the machines have it: its checks running, or the agent reviewing the commit it’s at', () => {
    expect(at({ checks: fx.checks({ pending: 2, passed: 3 }) })).toBe('validating');
    expect(at({ merge: 'clean', autoReview: review({ holds: true }) })).toBe('validating');
    // A round on a PR of yours holds nothing, and is still the agent's turn.
    expect(at({ mine: true, autoReview: review({ own: true }) })).toBe('validating');
    // One of an earlier commit is done with this one.
    expect(at({ autoReview: review({ head: 'c1' }) })).toBe('review');
  });

  it('is Ready to merge once GitHub would merge it, approved or not where the repo asks for none', () => {
    expect(at({ state: 'approved', merge: 'clean', checks: fx.checks({ passed: 4 }) })).toBe('ready');
    expect(at({ state: 'open', merge: 'clean' })).toBe('ready');
    // GitHub has the last word: approved with passing checks can still be held back.
    expect(at({ state: 'approved', merge: 'blocked', checks: fx.checks({ passed: 4 }) })).toBe('review');
    expect(at({ state: 'approved', merge: 'unknown' })).toBe('review');
  });

  it('is In review while it’s a person’s turn: a review to give, feedback, a failing check, a conflict', () => {
    expect(at({ reviewRequested: true })).toBe('review');
    expect(at({ state: 'changes' })).toBe('review');
    // A check that failed is someone's to see to, though others still run.
    expect(at({ checks: fx.checks({ failed: 1, pending: 2 }) })).toBe('review');
    expect(at({ state: 'approved', merge: 'conflicts' })).toBe('review');
  });

  it('puts a session, its worktree and a todo on it where the PR the board lists is, not the worktree’s copy', () => {
    const s = fx.session({ key: 'claude:claude:p', status: 'idle' });
    // The worktree's copy of the PR says less than the PRs view's: GitHub hasn't said it merges.
    const lane: Lane = fx.lane({ id: '/r/app', repoSlug: 'acme/app', sessions: [s], pr: fx.pr({ state: 'approved' }) });
    const prs = [fx.prItem({ state: 'approved', merge: 'clean', lanes: ['/r/app'] })];
    const todos: Todo[] = [fx.todo({ id: 'p', status: 'in-progress', worktrees: ['/r/app'] })];
    const todoItems = todos.map((t) => todoItem(t, new Map([[lane.id, lane]]), sessionHomes([lane]), new Map()));
    const subjects = subjectsOf({ todoItems, todos, sessionItems: sessionItemsOf([lane]), lanes: [lane], prs });
    expect(at2(subjects, 'session:claude:claude:p').stage).toBe('ready');
    expect(at2(subjects, 'worktree:/r/app').stage).toBe('ready');
    expect(at2(subjects, 'todo:p').stage).toBe('ready');
    // Without the board's copy, the worktree's says In review.
    expect(subjectsOf({ todoItems, todos, sessionItems: sessionItemsOf([lane]), lanes: [lane], prs: [] }).get('todo:p')?.stage).toBe('review');
  });

  it('takes a todo into the loop by the board’s copy of its PR, though its worktree’s still says draft', () => {
    const lane: Lane = fx.lane({ id: '/r/app', pr: fx.pr({ state: 'draft' }) });
    const t = fx.todo({ id: 'p', status: 'in-progress', worktrees: ['/r/app'] });
    const item = todoItem(t, new Map([[lane.id, lane]]), new Map(), new Map());
    expect(todoStage(item, [t], [fx.prItem({ state: 'approved', merge: 'clean' })])).toBe('ready');
    expect(todoStage(item, [t])).toBe('progress');
  });

  it('keeps a todo marked for review by hand, or whose PR merged before it was done, In review', () => {
    const draft: Lane = fx.lane({ id: '/r/draft', pr: fx.pr({ state: 'draft', checks: fx.checks({ pending: 1 }) }) });
    const merged: Lane = fx.lane({ id: '/r/merged', pr: fx.pr({ state: 'merged', number: 8 }) });
    const map = new Map([draft, merged].map((l) => [l.id, l]));
    const marked = fx.todo({ id: 'm', status: 'in-progress', review: { note: '', since: 1 }, worktrees: ['/r/draft'] });
    const late = fx.todo({ id: 'l', status: 'in-progress', worktrees: ['/r/merged'] });
    const item = (t: Todo) => todoItem(t, map, new Map(), new Map());
    expect(todoStage(item(marked), [marked])).toBe('review');
    expect(todoStage(item(late), [late])).toBe('review');
  });
});

const at2 = (subjects: Map<string, Subject>, card: string) => subjects.get(card) as Subject;

describe('what a thing on a board is to its rules', () => {
  const s = project();

  it('gives a session its todos’ labels, and a todo its sessions’ statuses and accounts', () => {
    expect(at(s, 'session:claude:claude:w')).toMatchObject({ ref: 'session', labels: ['l-rel'], statuses: ['working'], accounts: ['claude'], repos: ['acme/app'], stage: 'progress' });
    expect(at(s, 'todo:rel')).toMatchObject({ labels: ['l-rel'], statuses: ['working'], accounts: ['claude'], stage: 'progress' });
    expect(at(s, 'todo:plain')).toMatchObject({ statuses: [], stage: 'todo' });
    expect(at(s, 'worktree:/r/app')).toMatchObject({ labels: ['l-rel'], accounts: ['claude', 'codex'] });
    expect(at(s, 'pr:acme/app#7')).toMatchObject({ stage: 'review', accounts: ['codex'], repos: ['acme/app'] });
  });

  it('gives a session the labels you put on it, and its worktree too', () => {
    const own = fx.session({ key: 'claude:claude:o', labels: [{ id: 'l-ui', name: 'UI', color: 'teal' }] });
    const lane: Lane = fx.lane({ id: '/r/web', sessions: [own] });
    const todos: Todo[] = [fx.todo({ id: 'rel', labels: ['l-rel'], sessions: [own.key] })];
    const todoItems = todos.map((t) => todoItem(t, new Map([[lane.id, lane]]), sessionHomes([lane]), new Map()));
    const subjects = subjectsOf({ todoItems, todos, sessionItems: sessionItemsOf([lane]), lanes: [lane], prs: [] });
    expect(at(subjects, 'session:claude:claude:o').labels).toEqual(['l-rel', 'l-ui']);
    expect(at(subjects, 'worktree:/r/web').labels).toEqual(['l-rel', 'l-ui']);
    expect(matches({ labels: ['l-ui'], kinds: ['session'] }, at(subjects, 'session:claude:claude:o'), NOW)).toBe(true);
  });

  it('takes another Mac’s label for this one’s of its name, since rules name this Mac’s', () => {
    const here: Label[] = [{ id: 'l-bug', name: 'Bug', color: 'red' }];
    const there = fx.session({ key: '@a1b2c3d4e5:claude:claude:t', labels: [{ id: 'l-their-bug', name: 'bug', color: 'teal' }, { id: 'l-only-there', name: 'Spike', color: 'blue' }] });
    const lane: Lane = fx.lane({ id: '@a1b2c3d4e5:/r/web', sessions: [there] });
    const subjects = subjectsOf({ todoItems: [], todos: [], sessionItems: sessionItemsOf([lane]), lanes: [lane], prs: [], labels: here });
    expect(at(subjects, 'session:@a1b2c3d4e5:claude:claude:t').labels).toEqual(['l-bug', 'l-only-there']);
    expect(at(subjects, 'worktree:@a1b2c3d4e5:/r/web').labels).toEqual(['l-bug', 'l-only-there']);
    // One of this Mac's stays itself, though another here was renamed to its name.
    const twice: Label[] = [...here, { id: 'l-spike', name: 'Bug', color: 'blue' }];
    expect(ownLabels({ labels: [twice[1]] }, twice)).toEqual(['l-spike']);
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
