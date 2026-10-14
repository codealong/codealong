import { describe, expect, it } from 'vitest';
import * as fx from '../testing/fixtures';
import { NOW } from '../testing/fixtures';
import type { Lane, PrItem, Session, Todo } from '../types';
import { addRef, arrange, contains, FRAME_GAP, settleDrop, CARD_H, clearDoc, CARD_W, COLUMN_MAX, emptyDoc, entitiesOf, freeSpot, itemId, links, originLinks, outline, overlaps, readDoc, rectOf, DEFAULT_SCOPE, DEFAULT_TIDY, DEFAULT_INCLUDE, refId, sync, tidy, tidyPlan, type CanvasDoc, type FrameItem, type NoteItem, type RefItem } from './model';

const DAY = 24 * 60 * 60 * 1000;

const session = (key: string, fields: Partial<Session> = {}): Session => fx.session({ key, ...fields });
// A worktree on a branch of its own name, last active five seconds ago or when its sessions were.
const lane = (id: string, sessions: Session[], fields: Partial<Lane> = {}): Lane =>
  fx.lane({ id, sessions, branch: id, lastActivity: Math.max(NOW - 5000, ...sessions.map((s) => s.lastActivity)), ...fields });
const todo = (id: string, fields: Partial<Todo> = {}): Todo => fx.todo({ id, project: 'Acme', ...fields });

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

  it('takes nothing off once it is old, and never brings back what you took off', () => {
    let d = sync(emptyDoc(), e, NOW);
    d = { ...d, items: d.items.filter((it) => itemId(it) !== 'worktree:b'), hidden: ['worktree:b'] };
    expect(sync(d, e, NOW + 2 * DAY)).toBe(d);
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
    const again = tidy(d, e, NOW, new Set(), { ...DEFAULT_TIDY, layout: 'time' });
    expect(at(again, 'frame-mine')).toEqual(mine);
    expect(at(again, 'session:s1')).toMatchObject({ x: s1.x, y: s1.y });
    const autos = again.items.filter((it) => it.kind === 'frame' && it.auto);
    expect(autos.map((f) => (f as FrameItem).title)).toEqual(['Today']);
    // Laid out below what stayed.
    for (const c of refs(again)) if (itemId(c) !== 'session:s1') expect(c.y).toBeGreaterThan(mine.y + mine.h);
  });
});

describe('new work in frames', () => {
  const frames = (d: CanvasDoc) => d.items.filter((it): it is FrameItem => it.kind === 'frame');
  const frame = (d: CanvasDoc, id: string) => at(d, id) as FrameItem;
  const within = (f: FrameItem, c: RefItem) => {
    const r = rectOf(c);
    return r.x >= f.x && r.y >= f.y && r.x + r.w <= f.x + f.w && r.y + r.h <= f.y + f.h;
  };
  /** Every card fully in a frame, no two cards on each other, no two frames on each other. */
  const neat = (d: CanvasDoc) => {
    const cards = refs(d);
    for (const c of cards) expect(frames(d).some((f) => within(f, c)), itemId(c)).toBe(true);
    for (let i = 0; i < cards.length; i++) for (let j = i + 1; j < cards.length; j++) expect(overlaps(rectOf(cards[i]), rectOf(cards[j])), `${itemId(cards[i])} ${itemId(cards[j])}`).toBe(false);
    const fs = frames(d);
    for (let i = 0; i < fs.length; i++) for (let j = i + 1; j < fs.length; j++) expect(overlaps(fs[i], fs[j])).toBe(false);
  };
  const app = (n: number) => Array.from({ length: n }, (_, i) => lane(`a${i}`, [session(`s${i}`)]));
  const base = [...app(3), lane('site1', [session('x1')], { repo: 'site' }), lane('lib1', [session('y1')], { repo: 'lib' })];
  // Tidied a minute ago, so what the tests add counts as new since.
  const tidied = (lanes: Lane[], layout: 'worktree' | 'time' = 'worktree') => tidy(emptyDoc(), ents(lanes), NOW - 60_000, new Set(), { ...DEFAULT_TIDY, layout });

  it('puts a new worktree of a repo in the next free column of its frame, and the frame grows', () => {
    const d = tidied(base);
    const before = frame(d, 'auto:repo:app');
    const d2 = sync(d, ents([...base, lane('a9', [session('s9')])]), NOW);
    const f = frame(d2, 'auto:repo:app');
    expect(within(f, at(d2, 'worktree:a9') as RefItem)).toBe(true);
    expect(at(d2, 'session:s9')).toMatchObject({ x: (at(d2, 'worktree:a9') as RefItem).x });
    expect(f.w * f.h).toBeGreaterThan(before.w * before.h);
    neat(d2);
    // Everything inside the frame stayed put.
    for (const c of refs(d)) if (within(before, c)) expect(at(d2, itemId(c))).toMatchObject({ x: c.x, y: c.y });
  });

  it('starts a new row in the frame once the row is as wide as a tidy makes it', () => {
    const lanes = app(4);
    const d = tidied(lanes);
    const heads = refs(d).filter((c) => c.ref === 'worktree');
    let next = d;
    for (let i = 4; i < 12; i++) next = sync(next, ents([...lanes, ...app(i + 1).slice(4)]), NOW);
    neat(next);
    const ys = new Set(refs(next).filter((c) => c.ref === 'worktree').map((c) => c.y));
    expect(ys.size).toBeGreaterThan(1);
    const xs = refs(next).filter((c) => c.ref === 'worktree').map((c) => c.x);
    expect(Math.max(...xs) - Math.min(...xs)).toBeLessThan(8 * CARD_W + 8 * 28);
    for (const h of heads) expect(at(next, itemId(h))).toMatchObject({ x: h.x, y: h.y });
    // A session under a worktree of the first row moves the rows below down whole, level.
    const all = [...lanes, ...app(12).slice(4)];
    const more = sync(next, ents(all.map((l) => (l.id === 'a0' ? lane('a0', [...l.sessions, session('more')]) : l))), NOW);
    neat(more);
    const rows = (d: CanvasDoc) => [...new Set(refs(d).filter((c) => c.ref === 'worktree').map((c) => c.y))].length;
    expect(rows(more)).toBe(rows(next));
  });

  it('grows the frame for a new session, and moves what is below it down by as much', () => {
    // Enough repos that frames wrap into a second row.
    const repos = ['r1', 'r2', 'r3', 'r4', 'r5', 'r6'].map((r) => lane(`w-${r}`, Array.from({ length: 6 }, (_, i) => session(`${r}-${i}`)), { repo: r }));
    const d = tidied(repos);
    const fs = frames(d);
    const top = fs.reduce((a, b) => (b.y < a.y ? b : a));
    const below = fs.filter((f) => f.y > top.y + top.h && f.x < top.x + top.w && top.x < f.x + f.w);
    expect(below.length).toBeGreaterThan(0);
    const lane0 = repos.find((l) => `auto:repo:${l.repo}` === top.id)!;
    const grown = lane(lane0.id, [...lane0.sessions, session('new')], { repo: lane0.repo });
    const d2 = sync(d, ents(repos.map((l) => (l.id === lane0.id ? grown : l))), NOW);
    neat(d2);
    const dy = frame(d2, top.id).h - top.h;
    expect(dy).toBe(CARD_H.session + 10);
    for (const f of below) expect(frame(d2, f.id).y).toBe(f.y + dy);
    // The gap under the frame is what it was.
    const gapBefore = Math.min(...below.map((f) => f.y)) - (top.y + top.h);
    expect(Math.min(...below.map((f) => frame(d2, f.id).y)) - (top.y + top.h + dy)).toBe(gapBefore);
  });

  it('pushes a frame to the right over when a new column widens the one before it', () => {
    const d = tidied(base);
    const app0 = frame(d, 'auto:repo:app');
    const right = frames(d).filter((f) => f.x >= app0.x + app0.w && f.y < app0.y + app0.h && app0.y < f.y + f.h);
    let next = d;
    const lanes = [...base];
    for (let i = 3; i < 8; i++) {
      lanes.push(lane(`a${i}`, [session(`s${i}`)]));
      next = sync(next, ents(lanes), NOW);
    }
    neat(next);
    const f = frame(next, 'auto:repo:app');
    for (const r of right) expect(frame(next, r.id).x - (f.x + f.w)).toBeGreaterThanOrEqual(Math.min(r.x - (app0.x + app0.w), FRAME_GAP));
  });

  it('by day, puts new work in the latest day', () => {
    const d = tidied(base, 'time');
    const d2 = sync(d, ents([...base, lane('b1', [session('z1')], { repo: 'other' })]), NOW);
    neat(d2);
    expect(frames(d2).filter((f) => within(f, at(d2, 'worktree:b1') as RefItem)).map((f) => f.title)).toEqual(['Today']);
  });

  it('puts what has no frame in one column beside the frames, not in a line', () => {
    const d = tidied(base);
    let next = d;
    const lanes = [...base];
    for (const r of ['n1', 'n2', 'n3']) {
      lanes.push(lane(r, [], { repo: r }));
      next = sync(next, ents(lanes), NOW);
    }
    const loose = ['n1', 'n2', 'n3'].map((r) => at(next, `worktree:${r}`) as RefItem);
    expect(new Set(loose.map((c) => c.x)).size).toBe(1);
    const right = Math.max(...frames(d).map((f) => f.x + f.w));
    expect(loose[0].x).toBe(right + FRAME_GAP);
  });

  it('grows a frame to hold a card dropped over its edge', () => {
    const d = tidied(base);
    const f = frame(d, 'auto:repo:app');
    const card = refs(d).find((c) => within(f, c) && c.ref === 'session')!;
    const moved = { ...d, items: d.items.map((it) => (it === card ? { ...card, y: f.y + f.h - CARD_H.session / 2 - 4 } : it)) };
    const d2 = settleDrop(moved, new Set([itemId(card)]));
    expect(within(frame(d2, f.id), at(d2, itemId(card)) as RefItem)).toBe(true);
    expect(settleDrop(d2, new Set([itemId(card)]))).toBe(d2);
  });

  it('changes nothing when synced again', () => {
    const d = sync(tidied(base), ents([...base, lane('a9', [session('s9')])]), NOW);
    expect(sync(d, ents([...base, lane('a9', [session('s9')])]), NOW)).toBe(d);
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
    let d = tidy(emptyDoc(), e, NOW, new Set(), { ...DEFAULT_TIDY, sessions: '7d' });
    expect(refs(d).map(itemId).sort()).toEqual(['session:busy', 'session:fresh', 'session:week', 'todo:new', 'todo:stale', 'todo:worked', 'worktree:a', 'worktree:b']);
    d = { ...d, items: d.items.map((it) => (it.kind === 'ref' ? { ...it, kept: true } : it)) };
    const plan = tidyPlan(d, e, NOW, new Set(), { ...DEFAULT_TIDY, sessions: '1d', todos: '7d' });
    expect(plan.dropped.map(itemId).sort()).toEqual(['session:week', 'todo:stale', 'worktree:b']);
    const after = tidy(d, e, NOW, new Set(), { ...DEFAULT_TIDY, sessions: '1d', todos: '7d' });
    expect(refs(after).map(itemId).sort()).toEqual(['session:busy', 'session:fresh', 'todo:new', 'todo:worked', 'worktree:a']);
    // The options stick for the next tidy, and Bring in doesn't bring the old ones back, even with longer ages.
    expect(after.tidy).toMatchObject({ sessions: '1d', todos: '7d' });
    expect(sync({ ...after, scope: { ...after.scope, sessions: '7d', todos: 'open' } }, e, NOW + 1000)).toMatchObject({ items: after.items });
  });

  it('then brings in only what moves on after it', () => {
    const after = tidy(emptyDoc(), e, NOW, new Set(), { ...DEFAULT_TIDY, include: { ...DEFAULT_INCLUDE, todos: false } });
    const later = NOW + 60_000;
    const e2 = ents([lane('a', [fresh, busy, session('next', { lastPromptAt: later, lastActivity: later })]), lane('b', [week], { lastActivity: NOW - 5 * DAY })], todos);
    const d = sync(after, e2, later);
    expect(refs(d).map(itemId).filter((id) => !refs(after).some((it) => itemId(it) === id))).toEqual(['session:next']);
  });

  it('keeps a pinned worktree, and what sits in a frame you drew', () => {
    const d = tidy(emptyDoc(), e, NOW, new Set(), { ...DEFAULT_TIDY, sessions: '7d' });
    const w = at(d, 'session:week') as RefItem;
    const mine: FrameItem = { kind: 'frame', id: 'frame-mine', title: 'Later', x: w.x - 10, y: w.y - 10, w: CARD_W + 20, h: CARD_H.session + 20, color: 'blue' };
    const plan = tidyPlan({ ...d, items: [mine, ...d.items] }, e, NOW, new Set(['b']), DEFAULT_TIDY);
    expect(plan.dropped.map(itemId)).toEqual([]);
  });

  it('goes back as little as an hour', () => {
    const e2 = ents([lane('a', [session('now'), session('twoh', { lastActivity: NOW - 2 * 3600_000, lastPromptAt: NOW - 2 * 3600_000 })])]);
    const ids = (sessions: '1h' | '3h') => refs(tidy(emptyDoc(), e2, NOW, new Set(), { ...DEFAULT_TIDY, sessions })).filter((it) => it.ref === 'session').map(itemId).sort();
    expect(ids('1h')).toEqual(['session:now']);
    expect(ids('3h')).toEqual(['session:now', 'session:twoh']);
  });

  it('takes off an old session still open in a terminal tab', () => {
    const open = session('open', { live: true, status: 'unread', lastActivity: NOW - 5 * DAY, lastPromptAt: NOW - 5 * DAY });
    const e2 = ents([lane('a', [open], { lastActivity: NOW - 5 * DAY })]);
    expect(refs(tidy(emptyDoc(), e2, NOW))).toEqual([]);
  });

  it('shows no todos when asked for none', () => {
    const d = tidy(emptyDoc(), e, NOW, new Set(), { ...DEFAULT_TIDY, include: { ...DEFAULT_SCOPE.include, todos: false } });
    expect(refs(d).filter((it) => it.ref === 'todo')).toEqual([]);
  });
});

describe('bring in', () => {
  const lanes = [lane('a', [session('s1')]), lane('b', []), lane('c', [])];
  const pr = (key: string, fields: Partial<PrItem> = {}): PrItem => fx.prItem({ key, title: key, head: key, author: null, merge: 'clean', ...fields });
  const board = fx.board({ prs: [pr('mine', { mine: true, lanes: ['c'] }), pr('theirs'), pr('shut', { mine: true, state: 'merged' })] });
  const e = entitiesOf(board, lanes, [todo('t', { worktrees: ['b'] })], () => true);
  const ids = (include: Partial<typeof DEFAULT_SCOPE.include>) => refs(sync({ ...emptyDoc(), scope: { ...DEFAULT_SCOPE, include: { ...DEFAULT_SCOPE.include, ...include } } }, e, NOW)).map(itemId).sort();

  it('brings each kind in only when it is on', () => {
    expect(ids({})).toEqual(['session:s1', 'todo:t', 'worktree:a', 'worktree:b', 'worktree:c']);
    expect(ids({ sessions: false, todos: false })).toEqual(['worktree:a', 'worktree:b', 'worktree:c']);
  });

  it('without worktrees, brings a worktree only to head what is there', () => {
    expect(ids({ worktrees: false, todos: false })).toEqual(['session:s1', 'worktree:a']);
  });

  it('brings in your open PRs, or every open one, with the worktree that holds them', () => {
    expect(ids({ worktrees: false, sessions: false, todos: false, prs: 'mine' })).toEqual(['pr:mine', 'worktree:c']);
    expect(ids({ worktrees: false, sessions: false, todos: false, prs: 'open' })).toEqual(['pr:mine', 'pr:theirs', 'worktree:c']);
  });

  it('reads an old canvas: todos that were None are off, and its one scope is the tidy options too', () => {
    const d = readDoc({ items: [], scope: { sessions: '3d', todos: 'none', layout: 'time', clearedAt: NOW } });
    const filter = { include: { ...DEFAULT_SCOPE.include, todos: false }, sessions: '3d', todos: 'open' };
    expect(d.scope).toEqual({ ...filter, since: NOW, cleared: true });
    expect(d.tidy).toEqual({ ...filter, layout: 'time' });
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
    // A tidy brings the todo back, by its own options.
    expect(refs(tidy(d, e2, later)).map(itemId)).toContain('todo:t');
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

  it('ties a session to the one it came from, when both are on the canvas', () => {
    const e = ents([lane('a', [session('boss'), session('fork', { origin: { key: 'boss', how: 'fork' } }), session('sent', { origin: { key: 'boss', how: 'started' } }), session('lost', { origin: { key: 'gone', how: 'fork' } })])], []);
    const on = new Set(['boss', 'fork', 'lost'].map((k) => refId('session', k)));
    expect(originLinks(e, on)).toEqual([['session:boss', 'session:fork']]);
    on.add(refId('session', 'sent'));
    expect(originLinks(e, on)).toEqual([
      ['session:boss', 'session:fork'],
      ['session:boss', 'session:sent'],
    ]);
  });
});

describe('arrange', () => {
  const lanes = [lane('a', [session('s1'), session('s2')]), lane('b', [session('s3')]), lane('c', [session('s4')], { repo: 'site' })];
  const e = ents(lanes, [todo('t1', { worktrees: ['b'] })]);
  const tidied = tidy(sync(emptyDoc(), e, NOW), e, NOW);
  const frames = (d: CanvasDoc) => d.items.filter((it): it is FrameItem => it.kind === 'frame');
  const notes = (d: CanvasDoc) => d.items.filter((it): it is NoteItem => it.kind === 'note');
  const card = (d: CanvasDoc, id: string) => at(d, id) as RefItem;
  const s = (key: string) => ({ ref: 'session' as const, key });

  it('puts what each frame names in it, in columns by worktree in the order given, and the frames in order', () => {
    const r = arrange(
      tidied,
      e,
      [
        { title: 'Billing', note: 'Two sessions on the invoice fix.', cards: [s('s2'), { ref: 'worktree', key: 'a' }, s('s1')] },
        { title: 'Site', color: 'teal', cards: [s('s4'), s('s3')] },
      ],
      'keep',
      NOW,
    );
    expect(r.frames.map((f) => [f.title, f.cards])).toEqual([
      ['Billing', 3],
      ['Site', 2],
    ]);
    const [billing, site] = frames(r.doc).filter((f) => f.arranged);
    expect(billing).toMatchObject({ title: 'Billing', arranged: true });
    expect(billing.auto).toBeUndefined();
    expect(site.color).toBe('teal');
    expect(site.x).toBeGreaterThan(billing.x);
    // The worktree heads its column, then its sessions in the order named.
    const [w, s2, s1] = ['worktree:a', 'session:s2', 'session:s1'].map((id) => card(r.doc, id));
    expect([s2.x, s1.x]).toEqual([w.x, w.x]);
    expect(s2.y).toBeGreaterThan(w.y);
    expect(s1.y).toBeGreaterThan(s2.y);
    expect(s1.kept).toBe(true);
    // A column per worktree, the one named first on the left.
    expect(card(r.doc, 'session:s4').x).toBeLessThan(card(r.doc, 'session:s3').x);
    // The note goes across the frame's top, the cards under it.
    const [note] = notes(r.doc);
    expect(note).toMatchObject({ text: 'Two sessions on the invoice fix.', arranged: true });
    expect(contains(billing, note)).toBe(true);
    expect(w.y).toBeGreaterThanOrEqual(note.y + note.h);
    const solid = r.doc.items.filter((it) => it.kind !== 'frame').map(rectOf);
    for (let i = 0; i < solid.length; i++) for (let j = i + 1; j < solid.length; j++) expect(overlaps(solid[i], solid[j])).toBe(false);
  });

  it('lays out the rest below the frames as a tidy would, or takes it off', () => {
    const asked = [{ title: 'Billing', cards: [s('s1')] }];
    const kept = arrange(tidied, e, asked, 'keep', NOW);
    expect(kept.rest).toBe(refs(tidied).length - 1);
    expect(refs(kept.doc)).toHaveLength(refs(tidied).length);
    const billing = frames(kept.doc).find((f) => f.arranged) as FrameItem;
    const autos = frames(kept.doc).filter((f) => f.auto);
    expect(autos.length).toBeGreaterThan(0);
    for (const f of autos) expect(f.y).toBeGreaterThan(billing.y + billing.h);
    const off = arrange(tidied, e, asked, 'off', NOW);
    expect(refs(off.doc).map(itemId)).toEqual(['session:s1']);
    // Bring in doesn't bring them back until they move on.
    expect(sync(off.doc, e, NOW)).toBe(off.doc);
  });

  it('replaces the frames and notes the last arrange made, which a tidy keeps as yours', () => {
    const first = arrange(tidied, e, [{ title: 'Billing', note: 'Invoices', cards: [s('s1')] }], 'keep', NOW).doc;
    const billing = frames(first).find((f) => f.arranged) as FrameItem;
    const t = tidy(first, e, NOW + 1000);
    expect(at(t, billing.id)).toEqual(billing);
    expect(at(t, 'session:s1')).toEqual(at(first, 'session:s1'));
    expect(notes(t)).toEqual(notes(first));
    const second = arrange(t, e, [{ title: 'Site', cards: [s('s4')] }], 'keep', NOW + 2000).doc;
    expect(frames(second).filter((f) => f.arranged).map((f) => f.title)).toEqual(['Site']);
    expect(notes(second)).toEqual([]);
    // Named by no frame now, it's with the rest.
    expect(at(second, 'session:s1')).toBeTruthy();
  });

  it('leaves your frames and notes, and what is in them unless a frame names it', () => {
    const s1 = card(tidied, 'session:s1');
    const mine: FrameItem = { kind: 'frame', id: 'frame-mine', title: 'Mine', x: s1.x - 20, y: s1.y - 20, w: CARD_W + 40, h: CARD_H.session + 40, color: 'green' };
    const note: NoteItem = { kind: 'note', id: 'note-mine', text: 'Ask about this', x: -1000, y: -1000, w: 220, h: 140, color: 'gray' };
    const d = { ...tidied, items: [mine, note, ...tidied.items] };
    const r = arrange(d, e, [{ title: 'Billing', cards: [s('s2')] }], 'off', NOW).doc;
    expect(at(r, 'frame-mine')).toEqual(mine);
    expect(at(r, 'note-mine')).toEqual(note);
    expect(at(r, 'session:s1')).toEqual(s1);
    // Named, it leaves your frame for the one that names it.
    const again = arrange(d, e, [{ title: 'Billing', cards: [s('s1')] }], 'off', NOW).doc;
    expect(at(again, 'frame-mine')).toEqual(mine);
    expect(contains(mine, rectOf(card(again, 'session:s1')))).toBe(false);
  });

  it('leaves out what the board lacks, puts a card in the first frame that names it, and makes no empty frame', () => {
    const r = arrange(
      tidied,
      e,
      [
        { title: 'A', cards: [s('s1'), s('gone'), s('gone')] },
        { title: 'B', cards: [s('s1'), { ref: 'todo', key: 't1' }] },
        { title: 'Empty', cards: [] },
      ],
      'keep',
      NOW,
    );
    expect(r.missing).toEqual([s('gone')]);
    expect(r.frames.map((f) => [f.title, f.cards])).toEqual([
      ['A', 1],
      ['B', 1],
    ]);
    expect(refs(r.doc).filter((c) => itemId(c) === 'session:s1')).toHaveLength(1);
  });

  it('first brings on what Bring in would have, for a canvas read from its file behind the board', () => {
    // Saved before s4 moved on, by a window that has shown another project since.
    const behind = { ...tidied, items: tidied.items.filter((it) => itemId(it) !== 'session:s4'), scope: { ...tidied.scope, since: NOW - DAY } };
    const r = arrange(behind, e, [{ title: 'Billing', cards: [s('s1')] }], 'keep', NOW);
    expect(at(r.doc, 'session:s4')).toBeTruthy();
    // And from here on, Bring in starts after the arrange.
    expect(r.doc.scope.since).toBe(NOW);
  });

  it('brings on a named card that was off the canvas, and off the hidden list', () => {
    const d = { ...tidied, items: tidied.items.filter((it) => itemId(it) !== 'session:s3'), hidden: ['session:s3'] };
    const r = arrange(d, e, [{ title: 'B', cards: [s('s3')] }], 'keep', NOW).doc;
    expect(at(r, 'session:s3')).toMatchObject({ kept: true, title: lanes[1].sessions[0].title });
    expect(r.hidden).toEqual([]);
  });

  it('makes a note tall enough for its text', () => {
    const r = arrange(emptyDoc(), e, [{ title: 'Notes', note: 'word '.repeat(200), cards: [] }], 'keep', NOW).doc;
    const [n] = notes(r);
    // A thousand characters, about 65 to a line two columns wide.
    expect(n.h).toBeGreaterThanOrEqual(15 * 20);
    expect(contains(frames(r)[0], n)).toBe(true);
    expect(frames(r)[0].h).toBeGreaterThan(n.h);
  });
});

describe('outline', () => {
  it('says what each frame holds and who made it, and what the board no longer has', () => {
    const e = ents([lane('a', [session('s1')])]);
    let d = arrange(tidy(sync(emptyDoc(), e, NOW), e, NOW), e, [{ title: 'Billing', note: 'Invoices', cards: [{ ref: 'session', key: 's1' }] }], 'keep', NOW).doc;
    d = { ...d, items: [...d.items, { kind: 'ref', ref: 'session', key: 'old', x: -5000, y: -5000, title: 'Old one' }] };
    const o = outline(d, e);
    const [billing, app] = o.frames;
    expect(o.frames.map((f) => [f.title, f.by, f.frame])).toEqual([
      ['Billing', 'arrange', null],
      ['app', 'tidy', null],
    ]);
    expect(o.cards).toEqual([
      { ref: 'session', key: 's1', title: e.sessions.get('s1')?.session.title, frame: billing.id, gone: false },
      { ref: 'worktree', key: 'a', title: 'a', frame: app.id, gone: false },
      { ref: 'session', key: 'old', title: 'Old one', frame: null, gone: true },
    ]);
    expect(o.notes).toEqual([{ id: expect.any(String), text: 'Invoices', frame: billing.id, by: 'arrange' }]);
  });
});
