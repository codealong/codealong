import { describe, expect, it } from 'vitest';
import { applyOp, imageNames, insertImage, newTodo, nextColor, pieces, sessionHomes, snippet, todoIndex, todoInFilter, todoInQuery, todoItem, todoPrompt, todoSections, withoutImage, type TodoStore } from './todos';
import type { Label, Lane, Session, Todo } from './types';

const NOW = 1_790_000_000_000;

function todo(id: string, fields: Partial<Todo> = {}): Todo {
  return newTodo({ id, title: `Todo ${id}`, ...fields }, NOW);
}

function session(key: string, fields: Partial<Session> = {}): Session {
  return {
    key,
    id: key.split(':')[2] ?? key,
    shortId: 'abc1234',
    harness: 'claude',
    account: 'claude',
    surface: 'cli',
    status: 'idle',
    live: false,
    pid: null,
    title: `Session ${key}`,
    cwd: '/r/app',
    name: null,
    startedAt: NOW - 1000,
    statusSince: null,
    lastActivity: NOW - 1000,
    excerpt: null,
    branch: null,
    archived: false,
    attachCommand: null,
    stalledSince: null,
    resumeCommand: null,
    appAccount: false,
    openTarget: `s|${key}|focus`,
    visitor: null,
    headline: '',
    detail: null,
    ...fields,
  };
}

function lane(id: string, sessions: Session[], fields: Partial<Lane> = {}): Lane {
  return {
    id,
    project: 'Northwind',
    repo: 'app',
    repoSlug: 'northwindai/app',
    path: id,
    callsign: id.split('/').pop() ?? id,
    origin: 'script',
    branch: 'main',
    detached: null,
    bay: 'idle',
    glyph: 'idle',
    parked: false,
    hidden: false,
    headline: '',
    detail: null,
    sessions,
    pr: null,
    git: null,
    lastActivity: NOW,
    targets: [],
    defaultTarget: null,
    ...fields,
  };
}

const store = (...todos: Todo[]): TodoStore => ({ todos, labels: [] });

describe('applyOp', () => {
  it('puts new todos on top, and a todo made from a session in progress', () => {
    const s = applyOp(store(todo('a')), { op: 'create', todo: todo('b', { sessions: ['claude:claude:s1'] }) }, NOW);
    expect(s.todos.map((t) => t.id)).toEqual(['b', 'a']);
    expect(s.todos[0].status).toBe('in-progress');
    // The same id twice is one todo.
    expect(applyOp(s, { op: 'create', todo: todo('b') }, NOW)).toBe(s);
  });

  it('keeps done time with the status', () => {
    let s = applyOp(store(todo('a')), { op: 'update', id: 'a', patch: { status: 'done' } }, NOW + 5);
    expect(s.todos[0].doneAt).toBe(NOW + 5);
    s = applyOp(s, { op: 'update', id: 'a', patch: { title: 'Renamed' } }, NOW + 9);
    expect(s.todos[0]).toMatchObject({ title: 'Renamed', doneAt: NOW + 5, updatedAt: NOW + 9 });
    s = applyOp(s, { op: 'update', id: 'a', patch: { status: 'todo' } }, NOW + 10);
    expect(s.todos[0].doneAt).toBeNull();
  });

  it('links sessions and worktrees, and unlinking drops a start waiting there', () => {
    let s = applyOp(store(todo('a')), { op: 'link', id: 'a', session: 'codex:codex:t1', linked: true }, NOW);
    expect(s.todos[0]).toMatchObject({ sessions: ['codex:codex:t1'], status: 'in-progress' });
    s = applyOp(s, { op: 'link', id: 'a', worktree: '/r/app', linked: true }, NOW);
    s = { ...s, todos: [{ ...s.todos[0], pending: [{ lane: '/r/app', harness: 'codex', account: 'codex', at: NOW }] }] };
    s = applyOp(s, { op: 'link', id: 'a', worktree: '/r/app', linked: false }, NOW);
    expect(s.todos[0]).toMatchObject({ worktrees: [], pending: [], sessions: ['codex:codex:t1'] });
  });

  it('deletes, restores in place and reorders', () => {
    const s = store(todo('a'), todo('b'), todo('c'));
    const gone = applyOp(s, { op: 'delete', id: 'b' }, NOW);
    expect(gone.todos.map((t) => t.id)).toEqual(['a', 'c']);
    expect(applyOp(gone, { op: 'restore', todo: s.todos[1], index: 1 }, NOW).todos.map((t) => t.id)).toEqual(['a', 'b', 'c']);
    expect(applyOp(s, { op: 'order', ids: ['c', 'a'] }, NOW).todos.map((t) => t.id)).toEqual(['c', 'a', 'b']);
  });
});

describe('the list', () => {
  const working = session('claude:claude:w', { status: 'working', live: true, lastActivity: NOW });
  const idle = session('claude:claude:i', { lastActivity: NOW - 50_000 });
  const visitor = session('claude:claude:w', { visitor: 'worked here' });
  const lanes = [lane('/r/app', [working, idle]), lane('/r/app3', [visitor], { pr: { number: 7, title: 'x', url: '', state: 'open', checks: { passed: 0, failed: 2, pending: 0, skipped: 0, failing: [] }, updatedAt: NOW, mergedAt: null, closedAt: null } })];
  const byId = new Map(lanes.map((l) => [l.id, l]));
  const homes = sessionHomes(lanes);
  const bug: Label = { id: 'l1', name: 'Bug', color: 'red' };
  const labels = new Map([[bug.id, bug]]);

  it('places a todo’s sessions under their own worktrees, next to the ones it links', () => {
    const item = todoItem(todo('a', { worktrees: ['/r/app3'], sessions: ['claude:claude:i', 'claude:claude:w'], labels: ['l1', 'gone'] }), byId, homes, labels);
    expect(item.places.map((p) => [p.lane.id, p.linked, p.sessions.map((s) => s.key)])).toEqual([
      ['/r/app3', true, []],
      ['/r/app', false, ['claude:claude:w', 'claude:claude:i']],
    ]);
    expect(item.lane?.id).toBe('/r/app3');
    expect(item.labels).toEqual([bug]);
    expect(item.lastActivity).toBe(NOW);
    expect(todoInFilter(item, 'working')).toBe(true);
    expect(todoInFilter(item, 'failing')).toBe(true);
    expect(todoInFilter(item, 'needs')).toBe(false);
    expect(todoInQuery(item, 'bug')).toBe(true);
    expect(todoInQuery(item, 'app3')).toBe(true);
    expect(todoInQuery(item, 'nothing like it')).toBe(false);
  });

  it('shows In progress and Todo in your order, then Done, latest first', () => {
    const items = [
      todo('a', { status: 'done', doneAt: NOW - 10 }),
      todo('b'),
      todo('c', { status: 'in-progress' }),
      todo('d', { status: 'done', doneAt: NOW }),
      todo('e'),
    ].map((t) => todoItem(t, byId, homes, labels));
    expect(todoSections(items).map((s) => [s.id, s.rows.map((r) => r.todo.id)])).toEqual([
      ['in-progress', ['c']],
      ['todo', ['b', 'e']],
      ['done', ['d', 'a']],
    ]);
  });

  it('finds the todos on each session and worktree', () => {
    const a = todo('a', { sessions: ['claude:claude:w'] });
    const b = todo('b', { worktrees: ['/r/app3'] });
    const { bySession, byLane } = todoIndex([a, b], homes);
    expect(bySession.get('claude:claude:w')).toEqual([a]);
    // The session's own worktree, not the one it visited.
    expect(byLane.get('/r/app')).toEqual([a]);
    expect(byLane.get('/r/app3')).toEqual([b]);
  });
});

describe('prompts and images', () => {
  const shot = '0123456789abcdef.png';

  it('asks with the title, then the description', () => {
    expect(todoPrompt(todo('a', { title: ' Fix the header ', description: '\nIt wraps.\n' }))).toBe('Fix the header\n\nIt wraps.');
    expect(todoPrompt(todo('a', { title: 'Fix it' }))).toBe('Fix it');
  });

  it('splits a description into text and its images', () => {
    const text = `Before\n![image](images/${shot})\nafter ![x](images/nope.png) and ![y](https://e.com/a.png)`;
    expect(pieces(text)).toEqual([
      { kind: 'text', text: 'Before\n' },
      { kind: 'image', name: shot, alt: 'image' },
      { kind: 'text', text: '\nafter ![x](images/nope.png) and ![y](https://e.com/a.png)' },
    ]);
    expect(imageNames(`${text} ![again](images/${shot})`)).toEqual([shot]);
    expect(snippet(text)).toBe('Before after and');
  });

  it('puts an image on a line of its own at the cursor, and takes it out again', () => {
    const r = insertImage('See this for the bug', 8, shot);
    expect(r.text).toBe(`See this\n![image](images/${shot})\n for the bug`);
    expect(r.text.slice(0, r.cursor)).toBe(`See this\n![image](images/${shot})\n`);
    expect(insertImage('', 0, shot).text).toBe(`![image](images/${shot})`);
    expect(withoutImage(`One\n\n![image](images/${shot})\n\nTwo`, shot)).toBe('One\n\nTwo');
  });

  it('gives new labels a color no label has yet', () => {
    expect(nextColor([])).toBe('red');
    expect(nextColor([{ id: 'a', name: 'A', color: 'red' }])).toBe('pink');
  });
});
