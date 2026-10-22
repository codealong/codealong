import { describe, expect, it } from 'vitest';
import { filesToAttach } from './attachInput';
import { sessionHomes } from './lanes';
import { applyOp, attachmentsIn, blockedBy, cantWaitOn, closedWord, dependents, inlines, isOpen, insertAttachments, itemPrimaryHolds, mergeAttachments, nextColor, panelTodos, pieces, primaryHolds, prInReview, sectionOf, snippet, splitAttachments, todoAgent, todoIndex, todoInFilter, todoItem, todoPr, todoPrompt, todosPrompt, todoSections, statusSection, todoSuggestions, waitingChange, waitsOn, withAttachments, worthSuggesting, withoutAttachment, type TodoPatch, type TodoStore } from './todos';
import * as fx from './testing/fixtures';
import { NOW } from './testing/fixtures';
import type { Label, Lane, Session, Todo } from './types';

const todo = (id: string, fields: Partial<Todo> = {}): Todo => fx.todo({ id, ...fields });
const session = (key: string, fields: Partial<Session> = {}): Session => fx.session({ key, ...fields });
const lane = (id: string, sessions: Session[], fields: Partial<Lane> = {}): Lane => fx.lane({ id, sessions, ...fields });

const store = (...todos: Todo[]): TodoStore => ({ todos, labels: [] });

describe('applyOp', () => {
  it('puts new todos on top, and a todo made from a session in progress', () => {
    const s = applyOp(store(todo('a')), { op: 'create', todo: todo('b', { sessions: ['claude:claude:s1'] }) }, NOW);
    expect(s.todos.map((t) => t.id)).toEqual(['b', 'a']);
    expect(s.todos[0].status).toBe('in-progress');
    // The same id twice is one todo.
    expect(applyOp(s, { op: 'create', todo: todo('b') }, NOW)).toBe(s);
  });

  it('marks a todo ready for review as the engine does, until it’s sent back, waits, starts again or is done', () => {
    const review = { note: ' the  bar ', since: 0 };
    let s = applyOp(store(todo('a', { waiting: { note: 'Wes', since: 1 } })), { op: 'update', id: 'a', patch: { review } }, NOW);
    // Its work is under way, and it no longer waits.
    expect(s.todos[0]).toMatchObject({ status: 'in-progress', waiting: null, review: { note: 'the bar', since: NOW } });
    expect(sectionOf(s.todos[0], s.todos)).toBe('review');
    s = applyOp(s, { op: 'update', id: 'a', patch: { review: { note: 'x', since: 0 } } }, NOW + 5);
    expect(s.todos[0].review?.since).toBe(NOW);
    expect(applyOp(s, { op: 'update', id: 'a', patch: { review: null } }, NOW + 6).todos[0].review).toBeNull();
    expect(applyOp(s, { op: 'update', id: 'a', patch: { waiting: { note: '', since: 0 } } }, NOW + 6).todos[0].review).toBeNull();
    expect(applyOp(s, { op: 'update', id: 'a', patch: { status: 'in-progress' } }, NOW + 6).todos[0].review).toBeNull();
    expect(applyOp(s, { op: 'update', id: 'a', patch: { status: 'done' } }, NOW + 6).todos[0].review).toBeNull();
    // A done one can't be marked.
    const done = applyOp(store(todo('b', { status: 'done' })), { op: 'update', id: 'b', patch: { review } }, NOW);
    expect(done.todos[0]).toMatchObject({ status: 'done', review: null });
  });

  it('takes a session’s worktree’s PR for its todo only while the session is on its branch', () => {
    const pr = fx.pr({ state: 'open' });
    const moved = fx.lane({ id: '/r/app', branch: 'next-work', pr, sessions: [fx.session({ key: 'claude:claude:s', branch: 'first-work' })] });
    const t = todo('t', { status: 'in-progress', sessions: ['claude:claude:s'] });
    const item = (l: typeof moved) => todoItem(t, new Map([[l.id, l]]), new Map(l.sessions.map((s) => [s.key, { lane: l, session: s }])), new Map());
    expect([todoPr(item(moved)), prInReview(item(moved)), todoInFilter(item(moved), ['review'])]).toEqual([null, false, false]);
    const here = { ...moved, sessions: [fx.session({ key: 'claude:claude:s', branch: 'next-work' })] };
    expect([todoPr(item(here)), prInReview(item(here))]).toEqual([pr, true]);
    // Linked to the worktree itself, its PR is the todo's whatever its sessions are on.
    expect(todoPr(todoItem({ ...t, worktrees: ['/r/app'] }, new Map([[moved.id, moved]]), new Map(), new Map()))).toEqual(pr);
    // Worktrees go in the order of their newest session, on the branch or not, as todos.rs
    // `pr_in_review` has it: A's draft comes before B's open PR.
    const on = (key: string, branch: string, at: number) => fx.session({ key, branch, lastActivity: at });
    const a = fx.lane({ id: '/r/a', branch: 'a', pr: fx.pr({ number: 1, state: 'draft' }), sessions: [on('claude:claude:s1', 'other', 300), on('claude:claude:s2', 'a', 100)] });
    const b = fx.lane({ id: '/r/b', branch: 'b', pr: fx.pr({ number: 2, state: 'open' }), sessions: [on('claude:claude:s3', 'b', 200)] });
    const three = todoItem(todo('three', { status: 'in-progress', sessions: ['claude:claude:s1', 'claude:claude:s2', 'claude:claude:s3'] }), new Map([a, b].map((l) => [l.id, l])), sessionHomes([a, b]), new Map());
    expect([todoPr(three)?.number, prInReview(three)]).toEqual([1, false]);
  });

  it('narrows to In review by its PR, its mark or a merge it waits on you over', () => {
    const marked = todoItem(todo('m', { status: 'in-progress', review: { note: '', since: 1 } }), new Map(), new Map(), new Map());
    const merged = fx.lane({ id: '/r/m', pr: fx.pr({ state: 'merged' }) });
    const waitsOnYou = todoItem(todo('w', { status: 'in-progress', worktrees: ['/r/m'] }), new Map([[merged.id, merged]]), new Map(), new Map());
    const plain = todoItem(todo('p', { status: 'in-progress' }), new Map(), new Map(), new Map());
    expect([marked, waitsOnYou, plain].map((r) => todoInFilter(r, ['review']))).toEqual([true, true, false]);
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

  it('makes the first session linked the primary one, until it’s unlinked or another is made it', () => {
    let s = applyOp(store(todo('a')), { op: 'link', id: 'a', session: 'c:1', linked: true }, NOW);
    expect(s.todos[0].primary).toEqual({ session: 'c:1', since: NOW });
    // Taking over from a session that stopped is the engine's to say, on its board.
    s = applyOp(s, { op: 'link', id: 'a', session: 'c:2', linked: true }, NOW + 1);
    expect(s.todos[0].primary?.session).toBe('c:1');
    s = applyOp(s, { op: 'primary', id: 'a', session: 'c:3' }, NOW + 2);
    expect(s.todos[0]).toMatchObject({ sessions: ['c:1', 'c:2', 'c:3'], primary: { session: 'c:3', since: NOW + 2 } });
    s = applyOp(s, { op: 'link', id: 'a', session: 'c:3', linked: false }, NOW + 3);
    expect(s.todos[0].primary).toBeNull();
    s = applyOp(s, { op: 'primary', id: 'a', session: 'c:1' }, NOW + 4);
    expect(applyOp(s, { op: 'primary', id: 'a', session: null }, NOW + 5).todos[0].primary).toBeNull();
  });

  it('archives a todo as the engine does, keeping its status, until it’s unarchived, reopened or started', () => {
    let s = applyOp(store(todo('a', { status: 'in-progress', review: { note: '', since: 1 } })), { op: 'update', id: 'a', patch: { archived: true } }, NOW);
    expect(s.todos[0]).toMatchObject({ status: 'in-progress', archivedAt: NOW, review: { note: '', since: 1 } });
    expect(sectionOf(s.todos[0], s.todos, true)).toBe('archived');
    expect(isOpen(s.todos[0])).toBe(false);
    // Archived again, it keeps the time it was.
    expect(applyOp(s, { op: 'update', id: 'a', patch: { archived: true } }, NOW + 5).todos[0].archivedAt).toBe(NOW);
    const back = applyOp(s, { op: 'update', id: 'a', patch: { archived: false } }, NOW + 5).todos[0];
    expect([back.archivedAt, sectionOf(back, [back])]).toEqual([null, 'review']);
    // Reopened or started, it's back; marked done, it stays archived.
    expect(applyOp(s, { op: 'update', id: 'a', patch: { status: 'todo' } }, NOW + 5).todos[0].archivedAt).toBeNull();
    expect(applyOp(s, { op: 'update', id: 'a', patch: { status: 'in-progress' } }, NOW + 5).todos[0].archivedAt).toBeNull();
    s = applyOp(s, { op: 'update', id: 'a', patch: { status: 'done' } }, NOW + 5);
    expect(s.todos[0]).toMatchObject({ status: 'done', archivedAt: NOW });
    // A todo not started that a session is linked to is started, and back too.
    const linked = applyOp(store(todo('b', { archivedAt: NOW })), { op: 'link', id: 'b', session: 'claude:claude:s1', linked: true }, NOW + 5).todos[0];
    expect([linked.status, linked.archivedAt]).toEqual(['in-progress', null]);
    // A new todo isn't made archived.
    expect(applyOp(store(), { op: 'create', todo: todo('c', { archivedAt: NOW }) }, NOW).todos[0].archivedAt).toBeNull();
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
  const lanes = [lane('/r/app', [working, idle]), lane('/r/app3', [visitor], { pr: fx.pr({ title: 'x', url: '', checks: fx.checks({ failed: 2 }), merge: 'blocked' }) })];
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
    expect(todoInFilter(item, ['working'])).toBe(true);
    expect(todoInFilter(item, ['work'])).toBe(true);
    expect(todoInFilter(item, ['needs'])).toBe(false);
  });

  it('shows In progress, Ready and Waiting in your order, then Done, latest first', () => {
    const todos = [
      todo('a', { status: 'done', doneAt: NOW - 10 }),
      todo('b'),
      todo('c', { status: 'in-progress' }),
      todo('d', { status: 'done', doneAt: NOW }),
      // Waits on a todo that isn't done.
      todo('e', { dependsOn: ['c'] }),
      // Waits only on a done todo and a deleted one.
      todo('f', { dependsOn: ['a', 'gone'] }),
      todo('g', { waiting: { note: 'Graham', since: NOW } }),
      // An agent is on it: In progress, whatever it waits on, unless it's marked.
      todo('h', { status: 'in-progress', dependsOn: ['b'] }),
      todo('i', { status: 'in-progress', waiting: { note: '', since: NOW } }),
      // Marked, but done.
      todo('j', { status: 'done', doneAt: NOW - 20, waiting: { note: 'x', since: NOW } }),
    ];
    const items = todos.map((t) => todoItem(t, byId, homes, labels));
    expect(todoSections(items, todos).map((s) => [s.id, s.label, s.rows.map((r) => r.todo.id)])).toEqual([
      ['in-progress', 'In progress', ['c', 'h']],
      ['ready', 'Ready', ['b', 'f']],
      ['waiting', 'Waiting', ['e', 'g', 'i']],
      ['done', 'Done', ['d', 'a', 'j']],
    ]);
    // What a todo waits on can be in another project, outside the rows shown.
    expect(todoSections([items[4]], todos).map((s) => s.id)).toEqual(['waiting']);
    expect(sectionOf(todo('k', { dependsOn: ['c'] }), [])).toBe('ready');
  });

  it('lists archived todos last, the latest archived first, whatever their status, and nothing waits on them', () => {
    const todos = [
      todo('a', { archivedAt: NOW - 10 }),
      todo('b', { status: 'done', doneAt: NOW, archivedAt: NOW }),
      todo('c', { dependsOn: ['a'] }),
      todo('d', { status: 'done', doneAt: NOW }),
    ];
    const items = todos.map((t) => todoItem(t, byId, homes, labels));
    expect(todoSections(items, todos).map((s) => [s.id, s.label, s.rows.map((r) => r.todo.id)])).toEqual([
      ['ready', 'Ready', ['c']],
      ['done', 'Done', ['d']],
      ['archived', 'Archived', ['b', 'a']],
    ]);
    expect(todoSections(items, todos, 'added', NOW).map((s) => s.id).slice(-2)).toEqual(['done', 'archived']);
    expect(blockedBy(todos[2], todos)).toEqual([]);
  });

  it('by when added, lists the todos not done under the day each was added, the newest first, then Done', () => {
    const now = new Date(2026, 9, 14, 12).getTime();
    const day = (n: number) => new Date(2026, 9, 14 - n, 9).getTime();
    const todos = [
      todo('a', { createdAt: day(2), status: 'in-progress' }),
      todo('b', { createdAt: day(0) }),
      todo('c', { createdAt: day(0) + 1000, waiting: { note: 'x', since: NOW } }),
      todo('d', { createdAt: day(0), status: 'done', doneAt: NOW }),
      todo('e', { createdAt: day(40) }),
    ];
    const items = todos.map((t) => todoItem(t, byId, homes, labels));
    const sections = todoSections(items, todos, 'added', now);
    expect(sections.map((s) => [s.id, s.rows.map((r) => r.todo.id)])).toEqual([
      ['today', ['c', 'b']],
      ['week', ['a']],
      ['older', ['e']],
      ['done', ['d']],
    ]);
    // Only a status's sections keep your order.
    expect(sections.map((s) => statusSection(s.id))).toEqual([false, false, false, true]);
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
    const footer = "This is Codealong todo a. When it's ready for me to review, say so with `codealong todo review a`. When the work is merged or I say it's done, mark it done with `codealong todo done a`.";
    expect(todoPrompt(todo('a', { title: ' Fix the header ', description: '\nIt wraps.\n' }))).toBe(`Fix the header\n\nIt wraps.\n\n${footer}`);
    expect(todoPrompt(todo('a', { title: 'Fix it' }))).toBe(`Fix it\n\n${footer}`);
  });

  it('asks one session for several todos, numbered, then says which they are', () => {
    // The same text as `todos_prompt`'s test in cli.rs.
    const a = todo('t-a', { title: ' Fix the header ', description: '\nIt wraps.\n' });
    const b = todo('t-b', { title: 'Add a logout button', description: '' });
    const c = todo('t-c', { title: '', description: 'No title.' });
    const footer = "When one is ready for me to review, say so with `codealong todo review <id>`. When the work on one is merged or I say it's done, mark it done with `codealong todo done <id>`.";
    expect(todosPrompt([a, b])).toBe(`Work on these 2 todos:\n\n1. Fix the header\n\nIt wraps.\n\n2. Add a logout button\n\nThese are Codealong todos t-a (1) and t-b (2). ${footer}`);
    expect(todosPrompt([a, b, c])).toContain('3. Untitled\n\nNo title.\n\nThese are Codealong todos t-a (1), t-b (2) and t-c (3).');
    expect(todosPrompt([a])).toBe(todoPrompt(a));
  });

  it('tells a session started on a todo of its primary session', () => {
    // The same text as `todo_prompt`'s test in cli.rs.
    const a = todo('t-a', { title: 'Fix the header', primary: { session: 'claude:claude:s1', since: 0 } });
    const footer = "This is Codealong todo t-a. When it's ready for me to review, say so with `codealong todo review t-a`. When the work is merged or I say it's done, mark it done with `codealong todo done t-a`.";
    expect(todoPrompt(a, true)).toBe(`Fix the header\n\n${footer}\n\nt-a's primary session is claude:claude:s1, which is running: send it what you find with \`codealong session send claude:claude:s1 "…"\` rather than do the same work.`);
    expect(todoPrompt(a, false)).toBe(`Fix the header\n\n${footer}\n\nSession claude:claude:s1 had t-a before you: \`codealong session show claude:claude:s1\` has its records.`);
    // Without word of whether it runs, or with no primary session, it says nothing of one.
    expect(todoPrompt(a)).toBe(`Fix the header\n\n${footer}`);
    expect(todoPrompt(todo('t-b', { title: 'Fix the header' }), true)).not.toContain('primary');
    expect(todosPrompt([todo('t-b'), a], (t) => t.id === 't-a')).toMatch(/\n\nt-a's primary session is claude:claude:s1, which is running: .* rather than do the same work\.$/);
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
    expect(withoutAttachment(`Before ![image](images/${shot}) after`, `images/${shot}`)).toBe('Before after');
    expect(withoutAttachment(`Before ![image](images/${shot})\nAfter`, `images/${shot}`)).toBe('Before\nAfter');
    expect(splitAttachments(`One\n\n![image](images/${shot})\n[Crash 1.log](${log})\n\nTwo`)).toEqual({
      text: 'One\n\nTwo',
      attachments: [{ ...img, name: 'image' }, { ...file, name: 'Crash 1.log' }],
    });
    expect(splitAttachments(`Before ![image](images/${shot}) after`)).toEqual({
      text: 'Before after',
      attachments: [{ ...img, name: 'image' }],
    });
    expect(withAttachments('One\n\nTwo', [img, file])).toBe(`One\n\nTwo\n\n![image](images/${shot})\n[Crash 1.log](${log})`);
    expect(withAttachments('', [img])).toBe(`![image](images/${shot})`);
    expect(mergeAttachments([img], [img, file])).toEqual([img, file]);
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

describe('a todo’s primary session', () => {
  const s1 = session('c:1', { live: true, lastActivity: NOW - 9 });
  const s2 = session('c:2', { live: true, lastActivity: NOW });
  const item = (t: Todo, sessions: Session[]) => {
    const lanes = [lane('/r/app', sessions)];
    return todoItem(t, new Map(lanes.map((l) => [l.id, l])), sessionHomes(lanes), new Map());
  };

  it('is what a todo opens while it runs, before a newer session', () => {
    const t = todo('a', { sessions: ['c:1', 'c:2'], primary: { session: 'c:1', since: NOW - 10 } });
    expect(item(t, [s1, s2]).primary?.session.key).toBe('c:1');
    expect(todoAgent(item(t, [s1, s2]))?.session.key).toBe('c:1');
    // Stopped or archived, it doesn't hold the todo: the running one opens, as without one.
    for (const stopped of [{ ...s1, live: false }, { ...s1, archived: true }]) {
      const r = item(t, [stopped, s2]);
      expect(r.primary).toBeNull();
      expect(itemPrimaryHolds(r, NOW)).toBe(false);
      expect(todoAgent(r)?.session.key).toBe('c:2');
    }
  });

  it('holds the todo for a moment after it took it, before the board lists it', () => {
    const t = todo('a', { sessions: ['c:9'], primary: { session: 'c:9', since: NOW } });
    expect(primaryHolds(t, new Map(), NOW + 60_000)).toBe(true);
    expect(primaryHolds(t, new Map(), NOW + 5 * 60_000)).toBe(false);
    expect(primaryHolds(todo('b'), new Map(), NOW)).toBe(false);
    expect(item(t, []).primary).toBeNull();
  });
});

describe("a session's panel", () => {
  it('lists todos under way first, then the done ones latest first', () => {
    const list = [todo('a', { status: 'done', doneAt: NOW - 3000 }), todo('b'), todo('c', { status: 'done', doneAt: NOW - 1000 }), todo('d', { status: 'in-progress' }), todo('e', { status: 'done', doneAt: null })];
    expect(panelTodos(list).map((t) => t.id)).toEqual(['d', 'b', 'c', 'a', 'e']);
    expect(list.map((t) => t.id)).toEqual(['a', 'b', 'c', 'd', 'e']);
    // An archived one goes with the done ones.
    expect(panelTodos([todo('f', { status: 'in-progress', archivedAt: NOW }), todo('g')]).map((t) => t.id)).toEqual(['g', 'f']);
  });

  it('says what the todos are when none is still to do, never calling an archived one done', () => {
    const done = todo('a', { status: 'done' });
    const shelved = todo('b', { status: 'in-progress', archivedAt: NOW });
    expect(closedWord([done, { ...done, archivedAt: NOW }])).toBe('done ');
    expect(closedWord([shelved])).toBe('archived ');
    expect(closedWord([done, shelved])).toBe('');
  });
});

describe('waiting', () => {
  const store = (t: Todo): TodoStore => ({ todos: [t], labels: [] });
  const update = (s: TodoStore, patch: TodoPatch, at: number) => applyOp(s, { op: 'update', id: s.todos[0].id, patch }, at);

  it('marks a todo, keeps when it began, and says it’s ready', () => {
    let s = update(store(todo('a')), { waiting: { note: ' Graham’s\n letter ', since: 0 } }, NOW + 5);
    expect(s.todos[0].waiting).toEqual({ note: 'Graham’s letter', since: NOW + 5 });
    s = update(s, { waiting: { note: 'the letter', since: 0 } }, NOW + 9);
    expect(s.todos[0].waiting).toEqual({ note: 'the letter', since: NOW + 5 });
    s = update(s, { waiting: null }, NOW + 10);
    expect(s.todos[0].waiting).toBeNull();
    // An undo puts a wait back with its own time.
    s = update(s, { waiting: { note: 'back', since: NOW + 2 } }, NOW + 11);
    expect(s.todos[0].waiting).toEqual({ note: 'back', since: NOW + 2 });
  });

  it('ends when it’s marked in progress or done, not when a session is linked', () => {
    const marked = update(store(todo('a')), { waiting: { note: '', since: 0 } }, NOW + 1);
    const linked = applyOp(marked, { op: 'link', id: 'a', session: 'claude:claude:s', linked: true }, NOW + 2);
    expect([linked.todos[0].status, linked.todos[0].waiting?.note]).toEqual(['in-progress', '']);
    expect(update(linked, { status: 'in-progress' }, NOW + 3).todos[0].waiting).toBeNull();
    const done = update(marked, { status: 'done' }, NOW + 3);
    expect(done.todos[0].waiting).toBeNull();
    expect(update(done, { waiting: { note: 'x', since: 0 } }, NOW + 4).todos[0].waiting).toBeNull();
    // The same change can say both.
    expect(update(marked, { status: 'in-progress', waiting: { note: 'y', since: 0 } }, NOW + 5).todos[0].waiting?.note).toBe('y');
  });

  it('starts a new todo’s wait when it’s made', () => {
    const made = applyOp({ todos: [], labels: [] }, { op: 'create', todo: todo('a', { waiting: { note: 'vendor', since: 3 } }) }, NOW + 7);
    expect(made.todos[0].waiting).toEqual({ note: 'vendor', since: NOW + 7 });
  });

  it('marks the ones not waiting yet, and leaves done ones out', () => {
    const all = [todo('a'), todo('b', { waiting: { note: '', since: NOW } }), todo('c', { status: 'done' })];
    const w = waitingChange(all, all);
    expect(w).toEqual({ label: 'Mark as waiting', ops: [{ op: 'update', id: 'a', patch: { waiting: { note: '', since: 0 } } }], undo: [], said: null });
    expect(waitingChange([all[2]], all)).toBeNull();
  });

  it('stops a mark, and a wait on todos not done once it isn’t started, with what puts it back', () => {
    const blocker = todo('x', { title: 'Ship the API' });
    const marked = todo('a', { title: 'Docs', waiting: { note: 'Sam', since: NOW } });
    const blocked = todo('b', { title: 'Release', dependsOn: ['x', 'y'] });
    const all = [blocker, marked, blocked, todo('y', { status: 'done' })];
    expect(waitingChange([marked], all)).toEqual({
      label: 'Stop waiting',
      ops: [{ op: 'update', id: 'a', patch: { waiting: null } }],
      undo: [{ op: 'update', id: 'a', patch: { waiting: { note: 'Sam', since: NOW }, dependsOn: [] } }],
      said: '“Docs” no longer waits.',
    });
    // It names the one todo it waits on, and keeps its wait on the done one.
    expect(waitingChange([blocked], all)).toEqual({
      label: 'Stop waiting on “Ship the API”',
      ops: [{ op: 'update', id: 'b', patch: { waiting: null, dependsOn: ['y'] } }],
      undo: [{ op: 'update', id: 'b', patch: { waiting: null, dependsOn: ['x', 'y'] } }],
      said: '“Release” no longer waits on “Ship the API”.',
    });
    // One an agent is on keeps its waits.
    const going = todo('c', { status: 'in-progress', dependsOn: ['x'], waiting: { note: '', since: NOW } });
    expect(waitingChange([going], [...all, going])?.ops).toEqual([{ op: 'update', id: 'c', patch: { waiting: null } }]);
    expect(waitingChange([marked, blocked], all)).toMatchObject({ label: 'Stop waiting', said: '2 todos no longer wait.' });
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

  it('refuses archived ones', () => {
    const a = todo('a');
    const b = todo('b', { archivedAt: NOW });
    expect(cantWaitOn(a, b, [a, b])).toBe('Archived');
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

describe('suggestions from Jev', () => {
  const labels: Label[] = [
    { id: 'l1', name: 'bug', color: 'red' },
    { id: 'l2', name: 'infra', color: 'blue' },
    { id: 'l3', name: 'ui', color: 'green' },
  ];

  it('offers what it is sure of and the todo lacks, and never a loop', () => {
    const me = todo('me', { project: '', labels: ['l1'], dependsOn: ['a'] });
    const a = todo('a');
    const b = todo('b');
    const loop = todo('loop', { dependsOn: ['me'] });
    const done = todo('done', { status: 'done' });
    const all = [me, a, b, loop, done];
    const s = todoSuggestions(
      {
        projects: [
          ['Acme', 0.9],
          ['Blog', 0.1],
        ],
        labels: [
          ['l1', 0.95],
          ['l2', 0.8],
          ['l3', 0.4],
        ],
        dependsOn: [
          ['a', 0.9],
          ['b', 0.8],
          ['loop', 0.9],
          ['done', 0.9],
          ['gone', 0.9],
        ],
      },
      me,
      labels,
      all,
      ['Acme', 'Blog'],
    );
    expect(s.project).toBe('Acme');
    expect(s.labels.map((l) => l.id)).toEqual(['l2']);
    expect(s.dependsOn.map((t) => t.id)).toEqual(['b']);
  });

  it('leaves the project alone when it is already there or Jev is unsure', () => {
    const me = todo('me', { project: 'Acme' });
    const none = { labels: [], dependsOn: [] };
    expect(todoSuggestions({ ...none, projects: [['Acme', 0.9]] }, me, labels, [me], ['Acme', 'Blog']).project).toBeNull();
    expect(todoSuggestions({ ...none, projects: [['Blog', 0.6]] }, me, labels, [me], ['Acme', 'Blog']).project).toBeNull();
  });

  it('asks once the title says something, or has a description or screenshot with it', () => {
    expect(worthSuggesting(todo('x', { title: 'Fix it' }))).toBe(false);
    expect(worthSuggesting(todo('x', { title: '', description: '![image](images/0123456789abcdef.png)' }))).toBe(false);
    expect(worthSuggesting(todo('x', { title: 'Bar overlaps', description: '![image](images/0123456789abcdef.png)' }))).toBe(true);
    expect(worthSuggesting(todo('x', { title: 'Retry failed export jobs' }))).toBe(true);
  });
});
