import { describe, expect, it } from 'vitest';
import { newTodo } from '../todos';
import type { Lane, Session, Todo } from '../types';
import { addRef, CARD_H, clearDoc, unclear, CARD_W, COLUMN_MAX, emptyDoc, entitiesOf, freeSpot, itemId, links, overlaps, readDoc, rectOf, DEFAULT_SCOPE, refId, sync, tidy, tidyPlan, type CanvasDoc, type FrameItem, type RefItem } from './model';

const NOW = 1_790_000_000_000;
const DAY = 24 * 60 * 60 * 1000;

function session(key: string, fields: Partial<Session> = {}): Session {
  return {
    key,
    id: key,
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
    callsign: id,
    origin: 'script',
    branch: id,
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
    lastActivity: Math.max(NOW - 5000, ...sessions.map((s) => s.lastActivity)),
    targets: [],
    defaultTarget: null,
    ...fields,
  };
}

function todo(id: string, fields: Partial<Todo> = {}): Todo {
  return newTodo({ id, title: `Todo ${id}`, project: 'Northwind', ...fields }, NOW);
}

function ents(lanes: Lane[], todos: Todo[] = []) {
  return entitiesOf(null, lanes, todos, () => true);
}

const refs = (d: CanvasDoc) => d.items.filter((it): it is RefItem => it.kind === 'ref');
const at = (d: CanvasDoc, id: string) => d.items.find((it) => itemId(it) === id);

describe('sync', () => {
  const a = lane('a', [session('s1'), session('s2', { lastActivity: NOW - 10 * DAY, lastPromptAt: NOW - 10 * DAY })]);
  const b = lane('b', [session('s3')], { repo: 'site' });
  const old = lane('old', [], { lastActivity: NOW - 10 * DAY });
  const e = ents([a, b, old], [todo('t1', { worktrees: ['a'] }), todo('t2', { status: 'done' })]);

  it('brings on recent worktrees, sessions and open todos, and nothing old', () => {
    const d = sync(emptyDoc(), e, NOW);
    expect(refs(d).map(itemId).sort()).toEqual(['session:s1', 'session:s3', 'todo:t1', 'worktree:a', 'worktree:b']);
  });

  it('puts a session and a todo under their worktree, clear of each other', () => {
    const d = sync(emptyDoc(), e, NOW);
    const w = at(d, 'worktree:a') as RefItem;
    const s = at(d, 'session:s1') as RefItem;
    const t = at(d, 'todo:t1') as RefItem;
    expect(s.x).toBe(w.x);
    expect(s.y).toBeGreaterThanOrEqual(w.y + CARD_H.worktree);
    expect(t.x).toBe(w.x);
    const cards = refs(d).map(rectOf);
    for (let i = 0; i < cards.length; i++) for (let j = i + 1; j < cards.length; j++) expect(overlaps(cards[i], cards[j])).toBe(false);
  });

  it('leaves what is there where it is, and says so by returning the same document', () => {
    const d = sync(emptyDoc(), e, NOW);
    expect(sync(d, e, NOW)).toBe(d);
    // A new session lands without moving the others.
    const a2 = lane('a', [...a.sessions, session('s4')]);
    const d2 = sync(d, ents([a2, b, old], [todo('t1', { worktrees: ['a'] })]), NOW);
    for (const it of d.items) expect(at(d2, itemId(it))).toMatchObject({ x: it.x, y: it.y });
    expect(at(d2, 'session:s4')).toBeTruthy();
  });

  it('lets untouched things go once they are old, keeps what you kept, and never brings back what you took off', () => {
    let d = sync(emptyDoc(), e, NOW);
    d = { ...d, items: d.items.map((it) => (itemId(it) === 'session:s3' ? { ...it, kept: true } : it)), hidden: ['worktree:b'] };
    d = { ...d, items: d.items.filter((it) => itemId(it) !== 'worktree:b') };
    const later = NOW + 2 * DAY;
    const d2 = sync(d, e, later);
    // Worktree a stays: its open todo is still on the canvas.
    expect(refs(d2).map(itemId)).toEqual(['worktree:a', 'session:s3', 'todo:t1']);
  });
});

describe('addRef and freeSpot', () => {
  it('finds room below what is in the way', () => {
    const d = addRef(emptyDoc(), ents([lane('a', [])]), 'worktree', 'a', { x: 0, y: 0 }, true);
    const spot = freeSpot(d.items, { x: 10, y: 10, w: CARD_W, h: CARD_H.session });
    expect(overlaps({ ...spot, w: CARD_W, h: CARD_H.session }, rectOf(d.items[0]))).toBe(false);
  });

  it('adds a thing once, kept, off the hidden list', () => {
    const e = ents([lane('a', [])]);
    const d = addRef({ ...emptyDoc(), hidden: ['worktree:a'] }, e, 'worktree', 'a', null, true);
    expect(d.hidden).toEqual([]);
    expect(d.items).toMatchObject([{ kind: 'ref', ref: 'worktree', key: 'a', kept: true, title: 'a' }]);
    expect(addRef(d, e, 'worktree', 'a', null, true)).toBe(d);
  });
});

describe('tidy', () => {
  const lanes = [lane('a', [session('s1'), session('s2')]), lane('b', [session('s3')]), lane('c', [], { repo: 'site' })];
  const e = ents(lanes, [todo('t1', { worktrees: ['b'] }), todo('loose')]);

  it('makes a frame per repo with each worktree in a column, and nothing overlapping', () => {
    const d = tidy(sync(emptyDoc(), e, NOW), e, NOW);
    const frames = d.items.filter((it): it is FrameItem => it.kind === 'frame');
    expect(frames.map((f) => f.title).sort()).toEqual(['Not in a worktree', 'app', 'site']);
    const cards = refs(d);
    for (const c of cards) expect(frames.some((f) => rectOf(c).x >= f.x && rectOf(c).y >= f.y && rectOf(c).x + CARD_W <= f.x + f.w && rectOf(c).y + rectOf(c).h <= f.y + f.h)).toBe(true);
    for (let i = 0; i < cards.length; i++) for (let j = i + 1; j < cards.length; j++) expect(overlaps(rectOf(cards[i]), rectOf(cards[j]))).toBe(false);
    const w = at(d, 'worktree:b') as RefItem;
    expect(at(d, 'session:s3')).toMatchObject({ x: w.x });
    expect(at(d, 'todo:t1')).toMatchObject({ x: w.x });
    for (const f of frames) for (const g of frames) if (f !== g) expect(overlaps(f, g)).toBe(false);
  });

  it('continues a long column beside its worktree rather than down the canvas', () => {
    const many = lane('main', Array.from({ length: 20 }, (_, i) => session(`m${i}`, { lastActivity: NOW - i * 1000, lastPromptAt: NOW - i * 1000 })));
    const e2 = ents([many]);
    const d = tidy(sync(emptyDoc(), e2, NOW), e2, NOW);
    const xs = new Map<number, number>();
    for (const c of refs(d)) if (c.ref === 'session') xs.set(c.x, (xs.get(c.x) ?? 0) + 1);
    expect([...xs.values()]).toEqual([COLUMN_MAX, COLUMN_MAX, 20 - 2 * COLUMN_MAX]);
    const w = at(d, 'worktree:main') as RefItem;
    // The continued columns start level with the first session, under the worktree's row.
    const tops = [...xs.keys()].map((x) => Math.min(...refs(d).filter((c) => c.x === x && c.ref === 'session').map((c) => c.y)));
    expect(new Set(tops)).toEqual(new Set([w.y + CARD_H.worktree + 10]));
  });

  it('keeps your frames and what is in them, and replaces its own', () => {
    let d = tidy(sync(emptyDoc(), e, NOW), e, NOW);
    const s1 = at(d, 'session:s1') as RefItem;
    const mine: FrameItem = { kind: 'frame', id: 'frame-mine', title: 'Billing', x: s1.x - 20, y: s1.y - 20, w: CARD_W + 40, h: CARD_H.session + 40, color: 'green' };
    d = { ...d, items: [mine, ...d.items] };
    const again = tidy(d, e, NOW, new Set(), { ...DEFAULT_SCOPE, layout: 'time' });
    expect(at(again, 'frame-mine')).toEqual(mine);
    expect(at(again, 'session:s1')).toMatchObject({ x: s1.x, y: s1.y });
    const autos = again.items.filter((it) => it.kind === 'frame' && it.auto);
    expect(autos.map((f) => (f as FrameItem).title)).toEqual(['Today']);
    // Laid out below what stayed.
    for (const c of refs(again)) if (itemId(c) !== 'session:s1') expect(c.y).toBeGreaterThan(mine.y + mine.h);
  });
});

describe('tidy with ages', () => {
  const fresh = session('fresh');
  const week = session('week', { lastActivity: NOW - 5 * DAY, lastPromptAt: NOW - 5 * DAY, status: 'unread' });
  const busy = session('busy', { lastActivity: NOW - 9 * DAY, status: 'working' });
  const lanes = [lane('a', [fresh, busy]), lane('b', [week], { lastActivity: NOW - 5 * DAY })];
  const todos = [todo('new'), todo('stale', { updatedAt: NOW - 20 * DAY }), todo('worked', { updatedAt: NOW - 20 * DAY, sessions: ['fresh'] })];
  const e = ents(lanes, todos);

  it('takes older sessions off, kept or not, but never busy ones', () => {
    let d = tidy(emptyDoc(), e, NOW, new Set(), { ...DEFAULT_SCOPE, sessions: '7d' });
    expect(refs(d).map(itemId).sort()).toEqual(['session:busy', 'session:fresh', 'session:week', 'todo:new', 'todo:stale', 'todo:worked', 'worktree:a', 'worktree:b']);
    d = { ...d, items: d.items.map((it) => (it.kind === 'ref' ? { ...it, kept: true } : it)) };
    const plan = tidyPlan(d, e, NOW, new Set(), { ...DEFAULT_SCOPE, sessions: '1d', todos: '7d' });
    expect(plan.dropped.map(itemId).sort()).toEqual(['session:week', 'todo:stale', 'worktree:b']);
    const after = tidy(d, e, NOW, new Set(), { ...DEFAULT_SCOPE, sessions: '1d', todos: '7d' });
    expect(refs(after).map(itemId).sort()).toEqual(['session:busy', 'session:fresh', 'todo:new', 'todo:worked', 'worktree:a']);
    // The ages stick: the next sync doesn't bring the old ones back.
    expect(after.scope).toMatchObject({ sessions: '1d', todos: '7d' });
    expect(sync(after, e, NOW)).toBe(after);
  });

  it('keeps a pinned worktree, and what sits in a frame you drew', () => {
    const d = tidy(emptyDoc(), e, NOW, new Set(), { ...DEFAULT_SCOPE, sessions: '7d' });
    const w = at(d, 'session:week') as RefItem;
    const mine: FrameItem = { kind: 'frame', id: 'frame-mine', title: 'Later', x: w.x - 10, y: w.y - 10, w: CARD_W + 20, h: CARD_H.session + 20, color: 'blue' };
    const plan = tidyPlan({ ...d, items: [mine, ...d.items] }, e, NOW, new Set(['b']), DEFAULT_SCOPE);
    expect(plan.dropped.map(itemId)).toEqual([]);
  });

  it('goes back as little as an hour', () => {
    const e2 = ents([lane('a', [session('now'), session('twoh', { lastActivity: NOW - 2 * 3600_000, lastPromptAt: NOW - 2 * 3600_000 })])]);
    const ids = (sessions: '1h' | '3h') => refs(tidy(emptyDoc(), e2, NOW, new Set(), { ...DEFAULT_SCOPE, sessions })).filter((it) => it.ref === 'session').map(itemId).sort();
    expect(ids('1h')).toEqual(['session:now']);
    expect(ids('3h')).toEqual(['session:now', 'session:twoh']);
  });

  it('takes off an old session still open in a terminal tab', () => {
    const open = session('open', { live: true, status: 'unread', lastActivity: NOW - 5 * DAY, lastPromptAt: NOW - 5 * DAY });
    const e2 = ents([lane('a', [open], { lastActivity: NOW - 5 * DAY })]);
    expect(refs(tidy(emptyDoc(), e2, NOW))).toEqual([]);
  });

  it('shows no todos when asked for none', () => {
    const d = tidy(emptyDoc(), e, NOW, new Set(), { ...DEFAULT_SCOPE, todos: 'none' });
    expect(refs(d).filter((it) => it.ref === 'todo')).toEqual([]);
  });
});

describe('clearing', () => {
  const idle = session('idle', { lastPromptAt: NOW - 3000, statusSince: NOW - 2000 });
  const running = session('running', { status: 'working', live: true, lastPromptAt: NOW - 3000, statusSince: NOW - 3000 });
  const e = ents([lane('a', [idle]), lane('b', [running])], [todo('t', { worktrees: ['a'], createdAt: NOW - 5000, updatedAt: NOW - 5000 })]);

  it('brings nothing straight back, not even what is running', () => {
    const d = clearDoc(sync(emptyDoc(), e, NOW), NOW);
    expect(d.items).toEqual([]);
    expect(sync(d, e, NOW + 1000)).toBe(d);
  });

  it('brings back what moves on afterwards, with its worktree', () => {
    const d = clearDoc(sync(emptyDoc(), e, NOW), NOW);
    const later = NOW + 60_000;
    const e2 = ents(
      [lane('a', [session('idle', { lastPromptAt: later, lastActivity: later })]), lane('b', [session('running', { status: 'unread', live: true, lastPromptAt: NOW - 3000, statusSince: later, lastActivity: later })])],
      [todo('t', { worktrees: ['a'], createdAt: NOW - 5000, updatedAt: NOW - 5000 })],
    );
    expect(refs(sync(d, e2, later)).map(itemId).sort()).toEqual(['session:idle', 'session:running', 'worktree:a', 'worktree:b']);
    // Including older work again brings the todo back too.
    const all = { ...d, scope: unclear(d.scope) };
    expect(refs(sync(all, e2, later)).map(itemId)).toContain('todo:t');
  });
});

describe('readDoc and links', () => {
  it('drops what does not read as canvas items', () => {
    const d = readDoc({ items: [{ kind: 'ref', ref: 'worktree', key: 'a', x: 1, y: 2 }, { kind: 'ref', ref: 'nope', key: 'b', x: 0, y: 0 }, { kind: 'note', id: 'n', x: 0, y: 0 }, null], hidden: ['x', 3], camera: { x: 1, y: 2, zoom: 0 } });
    expect(d.items).toHaveLength(1);
    expect(d.hidden).toEqual(['x']);
    expect(d.camera).toBeNull();
    expect(readDoc(null)).toEqual(emptyDoc());
  });

  it('ties sessions to their worktree and todos to what they are linked to', () => {
    const e = ents([lane('a', [session('s1')])], [todo('t1', { sessions: ['s1'], dependsOn: ['t2'] }), todo('t2')]);
    const on = new Set([refId('worktree', 'a'), refId('session', 's1'), refId('todo', 't1'), refId('todo', 't2')]);
    expect(links(e, on)).toEqual([
      ['session:s1', 'worktree:a'],
      ['todo:t1', 'session:s1'],
      ['todo:t1', 'todo:t2'],
    ]);
  });
});
