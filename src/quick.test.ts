import { describe, expect, it } from 'vitest';
import { ALL_PROJECTS, attentionRows, findSessions, findTodos, laneGroups, projectCounts, readyTodos, shortcutFromKey, shortcutLabel, TAKEN, titleMarks } from './quick';
import type { Board, Label, Lane, Session, Todo } from './types';

const NOW = 1_790_200_000_000;

const session = (key: string, status: Session['status'], at: number, extra: Partial<Session> = {}): Session =>
  ({ key, status, statusSince: at, lastActivity: at, title: key, account: 'claude', branch: null, visitor: null, archived: false, ...extra }) as Session;
const lane = (id: string, sessions: Session[], extra: Partial<Lane> = {}): Lane =>
  ({ id, sessions, hidden: false, project: 'Northwind', callsign: id, repo: 'atlas', branch: id, pr: null, origin: 'script', lastActivity: 0, ...extra }) as unknown as Lane;
const board = (lanes: Lane[], todos: Todo[] = [], labels: Label[] = []): Board => ({ lanes, todos, labels }) as unknown as Board;
const todo = (id: string, title: string, extra: Partial<Todo> = {}): Todo => ({
  id,
  project: 'Northwind',
  title,
  description: '',
  status: 'todo',
  labels: [],
  worktrees: [],
  sessions: [],
  createdAt: NOW - 1000,
  updatedAt: NOW - 1000,
  doneAt: null,
  pending: [],
  dependsOn: [],
  waiting: null,
  ...extra,
});

describe('attentionRows', () => {
  it('lists what needs you, then finished turns, the newest status first in each', () => {
    const b = board([
      lane('app2', [session('w', 'working', NOW), session('u1', 'unread', NOW - 10), session('i', 'idle', NOW)]),
      lane('app3', [session('n', 'needs', NOW - 90), session('e', 'error', NOW - 5), session('u2', 'unread', NOW - 1)]),
    ]);
    const { rows, counts } = attentionRows(b, ALL_PROJECTS);
    expect(rows.map((r) => [r.key, r.group])).toEqual([
      ['e', 'needs'],
      ['n', 'needs'],
      ['u2', 'unread'],
      ['u1', 'unread'],
    ]);
    expect(counts).toEqual({ needs: 2, unread: 2, working: 1 });
  });

  it('lists the working sessions when nothing waits on you', () => {
    const b = board([lane('app2', [session('w1', 'working', NOW - 5), session('w2', 'working', NOW), session('i', 'idle', NOW)])]);
    expect(attentionRows(b, ALL_PROJECTS).rows.map((r) => r.key)).toEqual(['w2', 'w1']);
  });

  it('keeps to the project, leaves out visitors and archived sessions, and stops at the limit', () => {
    const b = board([
      lane('app2', [session('a', 'needs', NOW), session('v', 'needs', NOW, { visitor: 'worked here' }), session('x', 'unread', NOW, { archived: true })]),
      lane('kestrel', [session('s', 'needs', NOW)], { project: 'Kestrel' }),
      lane('stray', [session('h', 'needs', NOW)], { project: 'Elsewhere', hidden: true }),
    ]);
    expect(attentionRows(b, 'Northwind').rows.map((r) => r.key)).toEqual(['a']);
    expect(attentionRows(b, ALL_PROJECTS).rows.map((r) => r.key).sort()).toEqual(['a', 's']);
    // Elsewhere's worktrees show only when you pick Elsewhere.
    expect(attentionRows(b, 'Elsewhere').rows.map((r) => r.key)).toEqual(['h']);
    const many = board([lane('app2', Array.from({ length: 12 }, (_, i) => session(`n${i}`, 'needs', NOW - i)))]);
    expect(attentionRows(many, ALL_PROJECTS).rows).toHaveLength(9);
    expect(attentionRows(many, ALL_PROJECTS).counts.needs).toBe(12);
  });
});

describe('findSessions', () => {
  const b = board([
    lane('app2', [
      session('s1', 'idle', NOW - 100, { title: 'Fix the login flake' }),
      session('s2', 'working', NOW - 50, { title: 'Login page redesign', account: 'claude2' }),
      session('s3', 'unread', NOW - 10, { title: 'Upgrade deps' }),
    ]),
    lane('login-flow', [session('s4', 'needs', NOW - 1000, { title: 'Tests for checkout' })], { branch: 'fix/login-flow' }),
  ]);

  it('needs every word somewhere: title, account, worktree, branch, repo or project', () => {
    expect(findSessions(b, ALL_PROJECTS, 'login').map((r) => r.key)).toEqual(['s2', 's1', 's4']);
    expect(findSessions(b, ALL_PROJECTS, 'login claude2').map((r) => r.key)).toEqual(['s2']);
    expect(findSessions(b, ALL_PROJECTS, 'LOGIN   flake').map((r) => r.key)).toEqual(['s1']);
    expect(findSessions(b, ALL_PROJECTS, 'northwind upgrade').map((r) => r.key)).toEqual(['s3']);
    expect(findSessions(b, ALL_PROJECTS, 'nothing like it')).toEqual([]);
    expect(findSessions(b, ALL_PROJECTS, '   ')).toEqual([]);
  });

  it('puts titles with every word first, then by status, then the latest', () => {
    // s4 needs you but has "login" only in its branch; the titles that say it come first.
    expect(findSessions(b, ALL_PROJECTS, 'login')[2].key).toBe('s4');
    // Among titles with the word, the working one before the idle one.
    expect(findSessions(b, ALL_PROJECTS, 'login').slice(0, 2).map((r) => r.key)).toEqual(['s2', 's1']);
  });

  it('marks where the words are in the title', () => {
    expect(findSessions(b, ALL_PROJECTS, 'login flake')[0].marks).toEqual([
      [8, 13],
      [14, 19],
    ]);
  });
});

describe('findTodos', () => {
  const lanes = [lane('app2', [session('s1', 'working', NOW, { title: 'Wire the MFA check', account: 'claude2' })])];
  const labels: Label[] = [{ id: 'l1', name: 'security', color: 'red' }];
  const todos = [
    todo('t1', 'Turn off MFA fallbacks', { updatedAt: NOW - 50 }),
    todo('t2', 'Rotate keys', { description: 'After the MFA work lands', status: 'in-progress', updatedAt: NOW - 500 }),
    todo('t3', 'Audit admin roles', { labels: ['l1'] }),
    todo('t4', 'Old MFA notes', { status: 'done', doneAt: NOW, updatedAt: NOW }),
    todo('t5', 'MFA for Kestrel', { project: 'Kestrel' }),
    todo('t6', 'Ship the MFA page', { waiting: { note: 'design review', since: NOW } }),
    todo('t7', 'Clean up', { sessions: ['s1'] }),
  ];
  const b = board(lanes, todos, labels);

  it('needs every word in the title, description, labels, project, or linked worktrees and sessions', () => {
    expect(findTodos(b, 'Northwind', 'security').map((r) => r.key)).toEqual(['todo:t3']);
    expect(findTodos(b, 'Northwind', 'claude2 wire').map((r) => r.key)).toEqual(['todo:t7']);
    expect(findTodos(b, 'Northwind', 'app2').map((r) => r.key)).toEqual(['todo:t7']);
    expect(findTodos(b, 'Northwind', 'nothing like it')).toEqual([]);
  });

  it('puts titles with every word first, then in progress, ready, waiting and done, and keeps to the project', () => {
    // t2 has MFA only in its description and t7 only in its session's title, so they come after
    // the titles, in progress before ready.
    expect(findTodos(b, 'Northwind', 'mfa').map((r) => r.key)).toEqual(['todo:t1', 'todo:t6', 'todo:t4', 'todo:t2', 'todo:t7']);
    expect(findTodos(b, ALL_PROJECTS, 'mfa').map((r) => r.key)).toContain('todo:t5');
    expect(findTodos(b, 'Kestrel', 'mfa').map((r) => r.key)).toEqual(['todo:t5']);
  });

  it('says which are waiting and where the words are', () => {
    const [first, waiting] = findTodos(b, 'Northwind', 'mfa');
    expect(first.waiting).toBe(false);
    expect(waiting.waiting).toBe(true);
    expect(first.marks).toEqual([[9, 12]]);
    expect(first.item.todo.id).toBe('t1');
    expect(findTodos(b, 'Northwind', 'clean')[0].item.sessions.map((x) => x.session.key)).toEqual(['s1']);
  });
});

describe('readyTodos', () => {
  const lanes = [lane('app2', [session('s1', 'idle', NOW)]), lane('app3', [])];
  const todos = [
    todo('t1', 'Started', { status: 'in-progress', sessions: ['s1'] }),
    todo('t2', 'First ready', { worktrees: ['app3'] }),
    todo('t3', 'Marked waiting', { waiting: { note: 'a reply', since: NOW } }),
    todo('t4', 'Blocked', { dependsOn: ['t2'] }),
    todo('t5', 'Second ready', { dependsOn: ['t6'] }),
    todo('t6', 'Finished', { status: 'done', doneAt: NOW }),
    todo('t7', 'Kestrel’s', { project: 'Kestrel' }),
    todo('t8', 'Third ready'),
    todo('t9', 'Fourth ready'),
  ];
  const b = board(lanes, todos);

  it('lists the first few todos ready to start, in your order, and counts them all', () => {
    const { rows, count } = readyTodos(b, 'Northwind');
    expect(rows.map((r) => r.key)).toEqual(['todo:t2', 'todo:t5', 'todo:t8']);
    expect(rows.every((r) => r.group === 'ready' && !r.waiting)).toBe(true);
    expect(count).toBe(4);
    // Where each would start, for its row and for ⏎.
    expect(rows[0].item.lane?.id).toBe('app3');
  });

  it('keeps to the project, and takes a limit', () => {
    expect(readyTodos(b, 'Kestrel').rows.map((r) => r.key)).toEqual(['todo:t7']);
    expect(readyTodos(b, ALL_PROJECTS, 2)).toMatchObject({ count: 5, rows: [{ key: 'todo:t2' }, { key: 'todo:t5' }] });
    expect(readyTodos(board(lanes), ALL_PROJECTS)).toEqual({ rows: [], count: 0 });
  });
});

describe('titleMarks', () => {
  it('merges words that overlap, in title order', () => {
    expect(titleMarks('Login page redesign', ['page', 'log'])).toEqual([
      [0, 3],
      [6, 10],
    ]);
    expect(titleMarks('abcdef', ['bcd', 'cde'])).toEqual([[1, 5]]);
    expect(titleMarks('abc', ['x'])).toEqual([]);
  });
});

describe('projectCounts', () => {
  it('counts what needs you and finished turns per project', () => {
    const b = board([lane('a', [session('n', 'needs', NOW), session('u', 'unread', NOW), session('w', 'working', NOW)]), lane('s', [session('e', 'error', NOW)], { project: 'Kestrel' })]);
    expect(projectCounts(b, ['Northwind', 'Kestrel', ALL_PROJECTS])).toEqual({
      Northwind: { needs: 1, unread: 1 },
      Kestrel: { needs: 1, unread: 0 },
      [ALL_PROJECTS]: { needs: 2, unread: 1 },
    });
  });
});

describe('laneGroups', () => {
  it('puts pinned worktrees first, then repos by their latest activity, each main checkout first', () => {
    const lanes = [
      lane('a1', [], { repo: 'a', lastActivity: 5 }),
      lane('a', [], { repo: 'a', origin: 'main', lastActivity: 1 }),
      lane('b1', [], { repo: 'b', lastActivity: 9 }),
      lane('p', [], { repo: 'b', lastActivity: 0 }),
    ];
    expect(laneGroups(lanes, ['p', 'gone']).map((g) => [g.label, g.lanes.map((l) => l.id)])).toEqual([
      ['Pinned', ['p']],
      ['b', ['b1']],
      ['a', ['a', 'a1']],
    ]);
    // Across projects, a repo's heading names its project.
    expect(laneGroups([lane('x', [], { repo: 'r' }), lane('y', [], { repo: 'r', project: 'Kestrel' })], []).map((g) => g.label).sort()).toEqual(['Northwind · r', 'Kestrel · r']);
  });
});

describe('shortcuts', () => {
  const key = (code: string, mods: Partial<Record<'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey', boolean>> = {}) => ({ code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods });

  it('shows config.toml’s shortcuts the way macOS writes them', () => {
    expect(shortcutLabel('Option+Shift+Space')).toBe('⌥⇧Space');
    expect(shortcutLabel('Cmd+Ctrl+K')).toBe('⌃⌘K');
    expect(shortcutLabel('alt+`')).toBe('⌥`');
    expect(shortcutLabel('CommandOrControl+Shift+KeyP')).toBe('⇧⌘P');
    expect(shortcutLabel('ctrl+alt+Digit1')).toBe('⌃⌥1');
    expect(shortcutLabel('F18')).toBe('F18');
    expect(shortcutLabel('')).toBeNull();
    expect(shortcutLabel(undefined)).toBeNull();
    expect(shortcutLabel('Option+A+B')).toBeNull();
  });

  it('records a key press as config.toml writes it', () => {
    expect(shortcutFromKey(key('Space', { altKey: true, shiftKey: true }))).toEqual({ shortcut: 'Option+Shift+Space' });
    expect(shortcutFromKey(key('KeyK', { metaKey: true, ctrlKey: true }))).toEqual({ shortcut: 'Ctrl+Cmd+K' });
    expect(shortcutFromKey(key('Digit1', { ctrlKey: true, altKey: true }))).toEqual({ shortcut: 'Ctrl+Option+1' });
    expect(shortcutFromKey(key('F18'))).toEqual({ shortcut: 'F18' });
    expect(shortcutFromKey(key('Space', { metaKey: true }))).toEqual({ shortcut: 'Cmd+Space' });
    // What the label shows for a recorded one is what TAKEN is keyed by.
    expect(TAKEN[shortcutLabel('Cmd+Space')!]).toBe('Spotlight');
    expect(TAKEN[shortcutLabel((shortcutFromKey(key('Space', { altKey: true })) as { shortcut: string }).shortcut)!]).toContain('ChatGPT');
  });

  it('waits while only modifiers are down, and turns away keys that would take typing or menus', () => {
    expect(shortcutFromKey(key('AltLeft', { altKey: true }))).toBeNull();
    expect(shortcutFromKey(key('KeyK'))).toHaveProperty('error');
    expect(shortcutFromKey(key('KeyK', { shiftKey: true }))).toHaveProperty('error');
    expect(shortcutFromKey(key('KeyK', { metaKey: true }))).toHaveProperty('error');
    expect(shortcutFromKey(key('Tab', { metaKey: true, shiftKey: true }))).toHaveProperty('error');
    expect(shortcutFromKey(key('IntlBackslash', { altKey: true }))).toHaveProperty('error');
    // ⇧⌘ with a key is allowed: apps like Maccy ship on ⇧⌘C.
    expect(shortcutFromKey(key('KeyC', { metaKey: true, shiftKey: true }))).toEqual({ shortcut: 'Shift+Cmd+C' });
  });
});
