import { describe, expect, it } from 'vitest';
import { filesToAttach } from './attachInput';
import { applyOp, attachmentsIn, blockedBy, cantWaitOn, dependents, inlines, insertAttachments, newTodo, nextColor, pieces, sessionHomes, snippet, todoIndex, todoInFilter, todoInQuery, todoItem, todoPrompt, todoSections, waitsOn, withoutAttachment, type TodoStore } from './todos';
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
    lastPromptAt: NOW - 1000,
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
  const lanes = [lane('/r/app', [working, idle]), lane('/r/app3', [visitor], { pr: { number: 7, title: 'x', url: '', state: 'open', checks: { passed: 0, failed: 2, pending: 0, skipped: 0, failing: [] }, merge: 'blocked', updatedAt: NOW, mergedAt: null, closedAt: null } })];
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
    expect(snippet(text)).toBe('Before after and');
  });

  const log = 'files/fedcba9876543210/Crash-report-1-.log';

  it('finds what a text has attached, images and files, once each', () => {
    const text = `See [Crash report (1).log](${log}) and\n![image](images/${shot})\nagain [Crash report (1).log](${log}), not [x](files/nope/x) or ![y](images/nope.png)`;
    expect(attachmentsIn(text)).toEqual([
      { kind: 'file', path: log, name: 'Crash report (1).log' },
      { kind: 'image', path: `images/${shot}`, name: 'image' },
    ]);
    expect(snippet(`Fails on save.\n[crash.log](${log})`)).toBe('Fails on save. crash.log');
  });

  it('shows links and files inside the text', () => {
    expect(inlines(`Log: [crash.log](${log}), see https://e.com/x. Done`)).toEqual([
      { kind: 'text', text: 'Log: ' },
      { kind: 'file', name: 'crash.log', path: log },
      { kind: 'text', text: ', see ' },
      { kind: 'url', url: 'https://e.com/x' },
      { kind: 'text', text: '. Done' },
    ]);
    // An image isn't a file link.
    expect(inlines(`![image](images/${shot})`)).toEqual([{ kind: 'text', text: `![image](images/${shot})` }]);
  });

  it('puts attachments on lines of their own at the cursor, and takes them out again', () => {
    const img = { kind: 'image' as const, path: `images/${shot}`, name: 'image.png' };
    const file = { kind: 'file' as const, path: log, name: 'Crash [1].log' };
    const r = insertAttachments('See this for the bug', 8, [img, file]);
    expect(r.text).toBe(`See this\n![image](images/${shot})\n[Crash 1.log](${log})\n for the bug`);
    expect(r.text.slice(0, r.cursor)).toBe(`See this\n![image](images/${shot})\n[Crash 1.log](${log})\n`);
    expect(insertAttachments('', 0, [img]).text).toBe(`![image](images/${shot})`);
    expect(withoutAttachment(`One\n\n![image](images/${shot})\n\nTwo`, `images/${shot}`)).toBe('One\n\nTwo');
    expect(withoutAttachment(`One\n[Crash 1.log](${log})\nTwo`, log)).toBe('One\nTwo');
  });

  it('pastes text over the pictures apps put next to it, and attaches files', () => {
    const paste = (files: File[], text: string) =>
      filesToAttach({ clipboardData: { files, items: [], getData: (t: string) => (t === 'text/plain' ? text : '') } } as unknown as Parameters<typeof filesToAttach>[0]);
    const picture = new File(['x'], 'image.png', { type: 'image/png' });
    const pdf = new File(['%PDF'], 'spec.pdf', { type: 'application/pdf' });
    expect(paste([picture], '')).toEqual([picture]);
    expect(paste([picture], 'A1\tB1')).toEqual([]);
    expect(paste([pdf], '')).toEqual([pdf]);
    expect(paste([pdf, picture], 'spec.pdf')).toEqual([pdf, picture]);
    expect(paste([], 'plain text')).toEqual([]);
  });

  it('gives new labels a color no label has yet', () => {
    expect(nextColor([])).toBe('red');
    expect(nextColor([{ id: 'a', name: 'A', color: 'red' }])).toBe('pink');
  });
});

describe('dependencies', () => {
  // a waits on b and c; b waits on d; c is done; e waits on a deleted todo.
  const todos = [
    todo('a', { dependsOn: ['b', 'c'] }),
    todo('b', { dependsOn: ['d'] }),
    todo('c', { status: 'done' }),
    todo('d'),
    todo('e', { dependsOn: ['gone'] }),
  ];
  const [a, b, c, d, e] = todos;

  it('is blocked by the open todos it waits on, in its order', () => {
    expect(blockedBy(a, todos).map((t) => t.id)).toEqual(['b']);
    expect(blockedBy(b, todos).map((t) => t.id)).toEqual(['d']);
    expect(blockedBy(d, todos)).toEqual([]);
    expect(blockedBy(e, todos)).toEqual([]);
  });

  it('treats a board without dependencies as none', () => {
    const old = { ...todo('x') } as Partial<Todo>;
    delete old.dependsOn;
    expect(blockedBy(old as Todo, todos)).toEqual([]);
    expect(dependents(old as Todo, [...todos, old as Todo])).toEqual([]);
  });

  it('follows waits through other todos', () => {
    expect(waitsOn('a', 'd', todos)).toBe(true);
    expect(waitsOn('d', 'a', todos)).toBe(false);
    expect(waitsOn('e', 'a', todos)).toBe(false);
  });

  it('lists the todos waiting on one', () => {
    expect(dependents(b, todos).map((t) => t.id)).toEqual(['a']);
    expect(dependents(d, todos).map((t) => t.id)).toEqual(['b']);
    expect(dependents(a, todos)).toEqual([]);
  });

  it('refuses itself, done ones, repeats and loops', () => {
    expect(cantWaitOn(d, d, todos)).not.toBeNull();
    expect(cantWaitOn(d, c, todos)).not.toBeNull();
    expect(cantWaitOn(a, b, todos)).not.toBeNull();
    expect(cantWaitOn(d, a, todos)).not.toBeNull();
    expect(cantWaitOn(d, e, todos)).toBeNull();
  });

  it('keeps each id once and never the todo itself', () => {
    const store: TodoStore = { todos, labels: [] };
    const next = applyOp(store, { op: 'update', id: 'd', patch: { dependsOn: ['e', 'd', 'e'] } }, NOW + 1);
    expect(next.todos.find((t) => t.id === 'd')?.dependsOn).toEqual(['e']);
    const made = applyOp(store, { op: 'create', todo: todo('f', { dependsOn: ['a', 'f', 'a'] }) }, NOW + 1);
    expect(made.todos[0].dependsOn).toEqual(['a']);
  });
});
