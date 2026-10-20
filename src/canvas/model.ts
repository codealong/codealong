// A board's canvas: where you put each worktree, session, todo and PR, and your notes and frames.
// One per board (boards/model.ts), kept by the engine in boards.json (boards.rs).
//
// The canvas is for remembering where work is, so positions hold still: nothing moves unless you
// move it or ask for a tidy. New work lands in the frame it belongs in, beside what it belongs to
// (a session under its worktree, a new worktree in the next free column); the frame grows to hold
// it, and what's below or beside the frame moves along whole to keep its gap.
//
// Two settings decide what's on it. Tidy's say what the canvas holds when you tidy: it lays the
// canvas out once, taking off what's older and bringing on what's in. Bring in's say what new work
// comes on by itself afterwards: whatever moves on after the last tidy or clear. Nothing leaves by
// itself; what's on stays, drawn as gone once the board no longer has it, until a tidy or you take
// it off. What comes on is what the board holds (`OnBoard`): what its rule takes in, and what was
// put on it by hand, which is always on the canvas; taking a card off the canvas takes it off the
// board. An agent can lay it out too, in frames by what the work is for (`arrange`, which
// `codealong board arrange` asks the main window for: boards/useBoardAsks.ts).

import { sessionHomes, unattended, type Placed } from '../lanes';
import { LABEL_COLORS, type Board, type Lane, type LabelColor, type PrItem, type Session, type Todo } from '../types';

export type RefKind = 'worktree' | 'session' | 'todo' | 'pr';

/** A board thing on the canvas, by its key: a lane id, a session key, a todo id, a PR key. */
export interface RefItem {
  kind: 'ref';
  ref: RefKind;
  key: string;
  x: number;
  y: number;
  /** You moved it or added it: it stays when it's no longer recent. */
  kept?: boolean;
  /** Its title when last seen, for when the board no longer has it. */
  title?: string;
}

export interface NoteItem {
  kind: 'note';
  id: string;
  text: string;
  x: number;
  y: number;
  w: number;
  h: number;
  color: LabelColor;
  /** Written by `codealong board arrange` across its frame's top: the next arrange replaces it
   *  with the frame. Editing it makes it yours. */
  arranged?: boolean;
}

export interface FrameItem {
  kind: 'frame';
  id: string;
  title: string;
  x: number;
  y: number;
  w: number;
  h: number;
  color: LabelColor;
  /** Made by a tidy; the next tidy replaces it. Renaming it makes it yours. */
  auto?: boolean;
  /** Made by `codealong board arrange`: a tidy leaves it, with what's in it, as one you drew,
   *  and the next arrange replaces it. Renaming it makes it yours. */
  arranged?: boolean;
}

export type CanvasItem = RefItem | NoteItem | FrameItem;

export interface Camera {
  x: number;
  y: number;
  zoom: number;
}

export interface CanvasDoc {
  version: 1;
  items: CanvasItem[];
  camera: Camera | null;
  /** What new work comes on by itself. */
  scope: Scope;
  /** What a tidy holds and how it lays it out: the last tidy's choices. */
  tidy: TidyOptions;
}

/** How far back finished sessions go on the canvas. Busy ones are always on it. */
export type SessionAge = '1h' | '3h' | '6h' | '1d' | '3d' | '7d' | 'all';
/** Which open todos are on the canvas: ones changed or worked on in the last week or month, or every one. */
export type TodoAge = '7d' | '30d' | 'open';
/** Which PRs come on by themselves: none, yours and ones waiting on your review, or every open one. */
export type PrScope = 'off' | 'mine' | 'open';

/** What kinds of things a tidy holds, or come onto the canvas by themselves. */
export interface Include {
  sessions: boolean;
  todos: boolean;
  /** Recent worktrees even with nothing of theirs on the canvas. Off, a worktree comes on only
   *  to head a session, todo or PR that's there. */
  worktrees: boolean;
  prs: PrScope;
}

/** Which things, and how far back. */
export interface Filter {
  include: Include;
  sessions: SessionAge;
  todos: TodoAge;
}

/** Bring in: what new work comes on by itself. */
export interface Scope extends Filter {
  /** When the canvas was last tidied or cleared: only what moves on after it comes on by itself. */
  since?: number;
  /** The last change was a clear, not a tidy. */
  cleared?: boolean;
}

/** Tidy: what the canvas holds after one, and how it's laid out. */
export interface TidyOptions extends Filter {
  layout: TidyMode;
}

export const DEFAULT_INCLUDE: Include = { sessions: true, todos: true, worktrees: true, prs: 'off' };
export const DEFAULT_SCOPE: Scope = { include: DEFAULT_INCLUDE, sessions: '1d', todos: 'open' };
export const DEFAULT_TIDY: TidyOptions = { include: DEFAULT_INCLUDE, sessions: '1d', todos: 'open', layout: 'worktree' };

export const PR_SCOPES: { id: PrScope; label: string; title: string }[] = [
  { id: 'off', label: 'Off', title: 'Only PRs you add' },
  { id: 'mine', label: 'Mine', title: 'Your open PRs and ones waiting on your review' },
  { id: 'open', label: 'All open', title: 'Every open PR in the project' },
];

export const SESSION_AGES: { id: SessionAge; label: string; title: string }[] = [
  { id: '1h', label: '1h', title: 'Active in the last hour' },
  { id: '3h', label: '3h', title: 'Active in the last 3 hours' },
  { id: '6h', label: '6h', title: 'Active in the last 6 hours' },
  { id: '1d', label: '24h', title: 'Active in the last 24 hours' },
  { id: '3d', label: '3d', title: 'Active in the last 3 days' },
  { id: '7d', label: '7d', title: 'Active in the last 7 days' },
  { id: 'all', label: 'All', title: 'Every session on the board: the last 14 days' },
];

export const TODO_AGES: { id: TodoAge; label: string; title: string }[] = [
  { id: '7d', label: '7d', title: 'Open todos changed or worked on in the last 7 days' },
  { id: '30d', label: '30d', title: 'Open todos changed or worked on in the last 30 days' },
  { id: 'open', label: 'All open', title: 'Every open todo' },
];

const DAY_MS = 24 * 60 * 60 * 1000;
const HOUR_MS = 60 * 60 * 1000;
const SESSION_MS: Record<SessionAge, number> = { '1h': HOUR_MS, '3h': 3 * HOUR_MS, '6h': 6 * HOUR_MS, '1d': DAY_MS, '3d': 3 * DAY_MS, '7d': 7 * DAY_MS, all: Infinity };
const TODO_MS: Record<TodoAge, number> = { '7d': 7 * DAY_MS, '30d': 30 * DAY_MS, open: Infinity };

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export const CARD_W = 248;
export const CARD_H: Record<RefKind, number> = { worktree: 60, session: 56, todo: 48, pr: 48 };
/** Space between cards in a column, and between columns. */
export const GAP = 10;
export const COL_GAP = 28;
/** Inside a frame, around its cards. */
export const FRAME_PAD = 24;
/** Between frames when a tidy packs them: room for a frame's name, which sits above it. */
export const FRAME_GAP = 160;
/** Cards in a column under a worktree before it continues in the next column. */
export const COLUMN_MAX = 8;

export function emptyDoc(): CanvasDoc {
  return { version: 1, items: [], camera: null, scope: DEFAULT_SCOPE, tidy: DEFAULT_TIDY };
}

/** A saved document as read back, with anything missing or malformed dropped. */
export function readDoc(raw: unknown): CanvasDoc {
  const doc = emptyDoc();
  if (!raw || typeof raw !== 'object') return doc;
  const r = raw as Partial<CanvasDoc>;
  const num = (v: unknown) => typeof v === 'number' && Number.isFinite(v);
  if (Array.isArray(r.items)) {
    doc.items = r.items.filter((it): it is CanvasItem => {
      if (!it || typeof it !== 'object' || !num(it.x) || !num(it.y)) return false;
      if (it.kind === 'ref') return typeof it.key === 'string' && ['worktree', 'session', 'todo', 'pr'].includes(it.ref);
      if (it.kind === 'note' || it.kind === 'frame') return typeof it.id === 'string' && num(it.w) && num(it.h);
      return false;
    });
  }
  // Before Tidy had its own settings, one scope held both, and its layout and ages were the tidy's.
  const sc = r.scope as (Partial<Scope> & { layout?: unknown; clearedAt?: unknown }) | undefined;
  if (sc && typeof sc === 'object') {
    const since = num(sc.since) ? (sc.since as number) : num(sc.clearedAt) ? (sc.clearedAt as number) : null;
    doc.scope = { ...readFilter(sc), ...(since != null ? { since } : {}), ...(sc.cleared === true || (since != null && !num(sc.since)) ? { cleared: true } : {}) };
  }
  const td = (r.tidy ?? sc) as (Partial<TidyOptions> & { layout?: unknown }) | undefined;
  if (td && typeof td === 'object') doc.tidy = { ...readFilter(td), layout: td.layout === 'time' ? 'time' : 'worktree' };
  if (r.camera && num(r.camera.x) && num(r.camera.y) && num(r.camera.zoom) && r.camera.zoom > 0) doc.camera = r.camera;
  return doc;
}

function readFilter(sc: Partial<Filter>): Filter {
  const inc = (sc.include ?? {}) as Partial<Include>;
  const flag = (v: unknown, d: boolean) => (typeof v === 'boolean' ? v : d);
  return {
    include: {
      sessions: flag(inc.sessions, true),
      // A canvas saved when "None" was a todo age has todos off.
      todos: flag(inc.todos, (sc.todos as string) !== 'none'),
      worktrees: flag(inc.worktrees, true),
      prs: PR_SCOPES.some((x) => x.id === inc.prs) ? (inc.prs as PrScope) : 'off',
    },
    sessions: SESSION_AGES.some((a) => a.id === sc.sessions) ? (sc.sessions as SessionAge) : DEFAULT_SCOPE.sessions,
    todos: TODO_AGES.some((a) => a.id === sc.todos) ? (sc.todos as TodoAge) : DEFAULT_SCOPE.todos,
  };
}

export function itemId(it: CanvasItem): string {
  return it.kind === 'ref' ? refId(it.ref, it.key) : it.id;
}

export function refId(ref: RefKind, key: string): string {
  return `${ref}:${key}`;
}

export function rectOf(it: CanvasItem): Rect {
  if (it.kind === 'ref') return { x: it.x, y: it.y, w: CARD_W, h: CARD_H[it.ref] };
  return { x: it.x, y: it.y, w: it.w, h: it.h };
}

export function overlaps(a: Rect, b: Rect, pad = 0): boolean {
  return a.x < b.x + b.w + pad && b.x < a.x + a.w + pad && a.y < b.y + b.h + pad && b.y < a.y + a.h + pad;
}

export function contains(outer: Rect, inner: Rect): boolean {
  const cx = inner.x + inner.w / 2;
  const cy = inner.y + inner.h / 2;
  return cx >= outer.x && cx <= outer.x + outer.w && cy >= outer.y && cy <= outer.y + outer.h;
}

export function bounds(rects: Rect[]): Rect | null {
  if (rects.length === 0) return null;
  let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const r of rects) {
    x0 = Math.min(x0, r.x);
    y0 = Math.min(y0, r.y);
    x1 = Math.max(x1, r.x + r.w);
    y1 = Math.max(y1, r.y + r.h);
  }
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/** What the board holds, by card id (`refId`): whether a card is on it, what was put on it by
 *  hand and when, which a tidy always keeps, and what was taken off it, which the canvas never
 *  shows. */
export interface OnBoard {
  has: (id: string) => boolean;
  added: ReadonlyMap<string, number>;
  removed: ReadonlySet<string>;
}

/** Everything, as a project's own board holds it until something is taken off. */
export const WHOLE: OnBoard = { has: () => true, added: new Map(), removed: new Set() };

/** What the canvas can show, from the board, for one project (or all of them). */
export interface Entities {
  lanes: Map<string, Lane>;
  sessions: Map<string, Placed>;
  todos: Map<string, Todo>;
  prs: Map<string, PrItem>;
}

/** `lanes` are the project's, as the window shows them; todos and PRs are picked by `inProject`,
 *  so a todo in no project is on All projects' boards alone, as it's in All projects' lists alone.
 *  Archived sessions are left out but `archived`, those the board has by hand (boardLists.ts
 *  `archivedOn`). */
export function entitiesOf(board: Board | null, lanes: Lane[], todos: Todo[], inProject: (project: string) => boolean, archived: readonly Placed[] = []): Entities {
  const sessions = new Map([...sessionHomes(lanes)].filter(([, p]) => !p.session.archived));
  for (const { lane, session } of archived) if (!sessions.has(session.key)) sessions.set(session.key, { lane, session });
  return {
    lanes: new Map(lanes.map((l) => [l.id, l])),
    sessions,
    todos: new Map(todos.filter((t) => inProject(t.project)).map((t) => [t.id, t])),
    prs: new Map((board?.prs ?? []).filter((p) => inProject(p.project)).map((p) => [p.key, p])),
  };
}

/** The worktree a thing belongs with: its own for a worktree or session, the first linked for a todo, the holding one for a PR. */
export function homeLane(e: Entities, ref: RefKind, key: string): string | null {
  switch (ref) {
    case 'worktree':
      return e.lanes.has(key) ? key : null;
    case 'session':
      return e.sessions.get(key)?.lane.id ?? null;
    case 'todo': {
      const t = e.todos.get(key);
      if (!t) return null;
      const linked = t.worktrees.find((id) => e.lanes.has(id));
      if (linked) return linked;
      const s = t.sessions.flatMap((k) => e.sessions.get(k) ?? []).sort((a, b) => b.session.lastActivity - a.session.lastActivity)[0];
      return s?.lane.id ?? null;
    }
    case 'pr':
      return e.prs.get(key)?.lanes.find((id) => e.lanes.has(id)) ?? null;
  }
}

export function titleOf(e: Entities, ref: RefKind, key: string): string | null {
  switch (ref) {
    case 'worktree':
      return e.lanes.get(key)?.callsign ?? null;
    case 'session':
      return e.sessions.get(key)?.session.title ?? null;
    case 'todo': {
      const t = e.todos.get(key);
      return t ? t.title || 'Untitled' : null;
    }
    case 'pr': {
      const p = e.prs.get(key);
      return p ? `#${p.number} ${p.title}` : null;
    }
  }
}

export function lastActivityOf(e: Entities, ref: RefKind, key: string): number {
  switch (ref) {
    case 'worktree':
      return e.lanes.get(key)?.lastActivity ?? 0;
    case 'session':
      return e.sessions.get(key)?.session.lastActivity ?? 0;
    case 'todo':
      return e.todos.get(key)?.updatedAt ?? 0;
    case 'pr':
      return e.prs.get(key)?.updatedAt ?? 0;
  }
}

/**
 * What a filter takes in of what the board holds, in the order it's placed (worktrees first, so
 * the rest can land beside them), as its Include says: sessions active within its age, and any
 * busy one (working, or waiting on you); open todos within theirs, counting their sessions'
 * activity; open PRs, yours or all of them; and worktrees active within the sessions' age (or
 * pinned), or home to any of the rest. With `cleared`, only what moved on after that time. What
 * was put on the board by hand is in whatever its kind and age, unless it was put there before
 * `cleared`: a clear, or an arrange that takes the rest off, isn't undone by it. With no time
 * (`cleared` 0, as a tidy has it) it's all in, a card added at no known time too.
 */
export function recentRefs(e: Entities, now: number, pinned: Set<string> = new Set(), scope: Filter = DEFAULT_SCOPE, cleared = 0, on: OnBoard = WHOLE): { ref: RefKind; key: string }[] {
  const inc = scope.include;
  const since = now - SESSION_MS[scope.sessions];
  const hand = (ref: RefKind, key: string) => (on.added.get(refId(ref, key)) ?? -1) >= cleared && !on.removed.has(refId(ref, key));
  const sessions = [...e.sessions.values()].filter(({ session: s }) => hand('session', s.key) || (inc.sessions && on.has(refId('session', s.key)) && sessionIn(s, since) && movedSince(s, cleared)));
  sessions.sort((a, b) => b.session.lastPromptAt - a.session.lastPromptAt);
  const todos = [...e.todos.values()].filter((t) => hand('todo', t.id) || (inc.todos && on.has(refId('todo', t.id)) && todoIn(e, t, now, scope.todos) && Math.max(t.createdAt, t.updatedAt) > cleared));
  const prs = [...e.prs.values()].filter(
    (p) => hand('pr', p.key) || (on.has(refId('pr', p.key)) && inc.prs !== 'off' && p.state !== 'merged' && p.state !== 'closed' && (inc.prs === 'open' || p.mine || p.reviewRequested) && p.updatedAt > cleared),
  );
  prs.sort((a, b) => b.updatedAt - a.updatedAt);
  const homes = new Set<string>();
  for (const s of sessions) homes.add(s.lane.id);
  for (const t of todos) {
    const home = homeLane(e, 'todo', t.id);
    if (home) homes.add(home);
  }
  for (const p of prs) {
    const home = homeLane(e, 'pr', p.key);
    if (home) homes.add(home);
  }
  const lanes = [...e.lanes.values()].filter(
    (l) => hand('worktree', l.id) || (on.has(refId('worktree', l.id)) && (homes.has(l.id) || (inc.worktrees && (pinned.has(l.id) || l.lastActivity >= since) && l.lastActivity > cleared))),
  );
  lanes.sort((a, b) => a.repo.localeCompare(b.repo) || b.lastActivity - a.lastActivity);
  return [
    ...lanes.map((l) => ({ ref: 'worktree' as const, key: l.id })),
    ...sessions.map((s) => ({ ref: 'session' as const, key: s.session.key })),
    ...todos.map((t) => ({ ref: 'todo' as const, key: t.id })),
    ...prs.map((p) => ({ ref: 'pr' as const, key: p.key })),
  ];
}

/** A finished session that was never archived is still "unread" or idle, often still open in a
 *  terminal tab, so being open doesn't count: only busy ones (working, or waiting on you) outlast the age.
 *  A helper or a headless run comes on by itself only once it needs you: the rest of the time
 *  it's its agent's or its script's. */
function sessionIn(s: Session, since: number): boolean {
  if (unattended(s)) return false;
  return s.status === 'working' || s.status === 'needs' || s.status === 'error' || s.lastActivity >= since;
}

/** Whether a session did something after the canvas was tidied or cleared: a new prompt, or a
 *  turn that ended or started waiting on you. A turn still running then isn't new. */
function movedSince(s: Session, cleared: number): boolean {
  return cleared === 0 || s.lastPromptAt > cleared || (s.statusSince ?? 0) > cleared;
}

/** Takes everything off the canvas, though not off its board; only what moves on from now comes
 *  back by itself, and what was put on the board by hand. */
export function clearDoc(doc: CanvasDoc, now: number): CanvasDoc {
  return { ...doc, items: [], scope: { ...doc.scope, since: now, cleared: true } };
}

function todoIn(e: Entities, t: Todo, now: number, age: TodoAge): boolean {
  if (t.status === 'done') return false;
  if (age === 'open') return true;
  const worked = Math.max(t.updatedAt, ...t.sessions.map((k) => e.sessions.get(k)?.session.lastActivity ?? 0));
  return worked >= now - TODO_MS[age];
}

/**
 * Brings the document up to date with the board: new work Bring in takes, that moved on since the
 * last tidy or clear, lands beside what it belongs to, and so does what was put on the board by
 * hand; what was taken off the board comes off, and cards note their current title. Nothing else
 * comes off; that's a tidy's job, or yours. Returns the same document when nothing changed.
 */
export function sync(doc: CanvasDoc, e: Entities, now: number, pinned: Set<string> = new Set(), on: OnBoard = WHOLE): CanvasDoc {
  let changed = false;
  const recent = recentRefs(e, now, pinned, doc.scope, doc.scope.since ?? 0, on);
  const items = doc.items.filter((it) => it.kind !== 'ref' || !on.removed.has(itemId(it)));
  if (items.length !== doc.items.length) changed = true;
  for (let i = 0; i < items.length; i++) {
    const it = items[i];
    if (it.kind !== 'ref') continue;
    const title = titleOf(e, it.ref, it.key);
    if (title && title !== it.title) {
      items[i] = { ...it, title };
      changed = true;
    }
  }
  const have = new Set(items.map(itemId));
  let next: CanvasDoc = changed ? { ...doc, items } : doc;
  for (const r of recent) {
    const id = refId(r.ref, r.key);
    if (have.has(id)) continue;
    next = addRef(next, e, r.ref, r.key, null, false);
    have.add(id);
  }
  return next;
}

/**
 * Puts a thing on the canvas: at `at` when given (a drop, a right-click), else in the frame it
 * belongs in, beside what it belongs with, and keeps it when `kept`. A frame grows to hold what
 * lands in it, and what's below or beside it moves along.
 */
export function addRef(doc: CanvasDoc, e: Entities, ref: RefKind, key: string, at: { x: number; y: number } | null, kept: boolean): CanvasDoc {
  const id = refId(ref, key);
  const existing = doc.items.find((it) => itemId(it) === id);
  if (existing) return doc;
  const title = titleOf(e, ref, key) ?? undefined;
  const item: RefItem = { kind: 'ref', ref, key, x: 0, y: 0, ...(kept ? { kept: true } : {}), ...(title ? { title } : {}) };
  const items = at ? dropAt(doc.items, item, at) : placeNear(doc, e, item);
  return { ...doc, items };
}

/** The rects things can't be put on top of: cards and notes, not frames, which hold things. */
function solids(items: CanvasItem[]): Rect[] {
  return items.filter((it) => it.kind !== 'frame').map(rectOf);
}

/** The nearest free spot at or below `want`, stepping down, then over a column at a time. */
export function freeSpot(items: CanvasItem[], want: Rect): { x: number; y: number } {
  const taken = solids(items);
  const free = (r: Rect) => !taken.some((t) => overlaps(r, t, GAP - 1));
  for (let col = 0; col < 40; col++) {
    const x = want.x + col * (CARD_W + COL_GAP);
    for (let step = 0; step < 200; step++) {
      const r = { ...want, x, y: want.y + step * 8 };
      if (free(r)) return { x: r.x, y: r.y };
    }
  }
  return { x: want.x, y: want.y };
}

/* ---- Frames that hold what lands in them ---- */

/** The frame a thing sits in: the smallest one holding its middle (and bigger than it, for a frame). */
export function parentOf(items: CanvasItem[], it: CanvasItem): FrameItem | null {
  const r = rectOf(it);
  let best: FrameItem | null = null;
  for (const f of items) {
    if (f.kind !== 'frame' || f === it || !contains(f, r)) continue;
    if (it.kind === 'frame' && f.w * f.h <= it.w * it.h) continue;
    if (!best || f.w * f.h < best.w * best.h) best = f;
  }
  return best;
}

/** What sits directly in a frame, or on the canvas outside every frame for null. */
function childrenOf(items: CanvasItem[], frame: string | null): CanvasItem[] {
  return items.filter((it) => (parentOf(items, it)?.id ?? null) === frame);
}

/** A thing and, for a frame, everything in it: what moves when it moves. */
function withContents(items: CanvasItem[], it: CanvasItem): Set<string> {
  const out = new Set([itemId(it)]);
  if (it.kind === 'frame') for (const o of items) if (o !== it && contains(it, rectOf(o))) out.add(itemId(o));
  return out;
}

type Axis = 'x' | 'y';
const lo = (r: Rect, a: Axis) => r[a];
const hi = (r: Rect, a: Axis) => r[a] + (a === 'x' ? r.w : r.h);
/** Whether two rects share some of the other axis: one is straight below (or beside) the other. */
const inLine = (r: Rect, s: Rect, a: Axis) => (a === 'x' ? r.y < s.y + s.h && s.y < r.y + r.h : r.x < s.x + s.w && s.x < r.x + r.w);

/**
 * Makes room for something that grew from `old` to `grown`, when a sibling is in the way past an
 * edge it grew across: what's past that edge moves away by one amount, each with what it holds,
 * so they keep their places relative to each other. The gap to the nearest keeps its size, up to
 * `gap`; one already wider takes up the growth first.
 */
function pushAside(items: CanvasItem[], siblings: Set<string>, old: Rect, grown: Rect, gap: number): CanvasItem[] {
  for (const a of ['y', 'x'] as Axis[]) {
    for (const sign of [1, -1]) {
      const grew = sign > 0 ? hi(grown, a) - hi(old, a) : lo(old, a) - lo(grown, a);
      if (grew <= 0) continue;
      const sibs = items.filter((it) => siblings.has(itemId(it)));
      const past = (r: Rect, from: Rect) => (sign > 0 ? lo(r, a) >= hi(from, a) - 0.5 : hi(r, a) <= lo(from, a) + 0.5);
      const first = sibs.filter((it) => past(rectOf(it), old) && inLine(rectOf(it), grown, a));
      if (first.length === 0) continue;
      const near = sign > 0 ? Math.min(...first.map((it) => lo(rectOf(it), a))) : Math.max(...first.map((it) => hi(rectOf(it), a)));
      const had = sign > 0 ? near - hi(old, a) : lo(old, a) - near;
      const by = sign > 0 ? hi(grown, a) + Math.min(had, gap) - near : near - (lo(grown, a) - Math.min(had, gap));
      if (by <= 0) continue;
      // Down (or up), everything past the edge moves, so rows stay level; across, only what's in
      // line, so the rows above and below stay where they are.
      const movers = new Set(a === 'y' ? sibs.filter((it) => past(rectOf(it), old)) : first);
      for (let more = a === 'x'; more; ) {
        more = false;
        for (const s of sibs) {
          if (movers.has(s)) continue;
          if ([...movers].some((m) => past(rectOf(s), rectOf(m)) && inLine(rectOf(s), rectOf(m), a))) {
            movers.add(s);
            more = true;
          }
        }
      }
      const moving = new Set([...movers].flatMap((m) => [...withContents(items, m)]));
      const d = sign * Math.round(by);
      items = items.map((it) => (moving.has(itemId(it)) ? { ...it, [a]: it[a] + d } : it));
    }
  }
  return items;
}

/** Grows a frame to hold `need` with its padding, making room around it, and its own frame to hold it.
 *  What's being put in the frame goes in after, or it would be pushed aside as something outside. */
export function fitFrame(items: CanvasItem[], id: string, need: Rect): CanvasItem[] {
  const f = items.find((it): it is FrameItem => it.kind === 'frame' && it.id === id);
  if (!f) return items;
  const old = rectOf(f);
  const want = bounds([old, { x: need.x - FRAME_PAD, y: need.y - FRAME_PAD, w: need.w + FRAME_PAD * 2, h: need.h + FRAME_PAD * 2 }]) as Rect;
  if (want.x === old.x && want.y === old.y && want.w === old.w && want.h === old.h) return items;
  const parent = parentOf(items, f);
  const siblings = new Set(childrenOf(items, parent?.id ?? null).filter((it) => it !== f).map(itemId));
  items = items.map((it) => (it === f ? { ...f, ...want } : it));
  items = pushAside(items, siblings, old, want, parent ? COL_GAP * 2 : FRAME_GAP);
  return parent ? fitFrame(items, parent.id, want) : items;
}

/**
 * After a drag: a card or note dropped with its middle in a frame but hanging over its edge makes
 * the frame grow to hold it.
 */
export function settleDrop(doc: CanvasDoc, moved: Set<string>): CanvasDoc {
  let items = doc.items;
  for (const id of moved) {
    const it = items.find((o) => itemId(o) === id);
    if (!it || it.kind === 'frame') continue;
    const f = parentOf(items, it);
    if (f) items = fitFrame(items, f.id, rectOf(it));
  }
  return items === doc.items ? doc : { ...doc, items };
}

/** How many columns a frame of `n` worktrees lays out across before it starts another row. */
const perRow = (n: number) => Math.max(1, Math.min(8, Math.ceil(Math.sqrt(n * 2.5))));

function dropAt(items: CanvasItem[], item: RefItem, at: { x: number; y: number }): CanvasItem[] {
  const spot = freeSpot(items, { w: CARD_W, h: CARD_H[item.ref], ...at });
  const placed = { ...item, ...spot };
  const f = parentOf(items, placed);
  return [...(f ? fitFrame(items, f.id, rectOf(placed)) : items), placed];
}

/**
 * Puts a card under the last card of a column (`head` and what's under it), moving what's below
 * in its frame down to make room, and growing the frame.
 */
function underColumn(items: CanvasItem[], item: RefItem, column: CanvasItem[]): CanvasItem[] {
  const head = column[0];
  const frame = parentOf(items, head);
  const siblings = childrenOf(items, frame?.id ?? null);
  const bottom = Math.max(...column.map((it) => rectOf(it).y + rectOf(it).h));
  const placed: RefItem = { ...item, x: head.x, y: bottom + GAP };
  const r = rectOf(placed);
  const next = pushAside(items, new Set(siblings.map(itemId)), { x: r.x, y: bottom, w: r.w, h: 0 }, { x: r.x, y: bottom, w: r.w, h: r.y + r.h - bottom }, COL_GAP * 2);
  if (!frame) return [...next, placed];
  // Held to what was in the frame before anything moved: what moved down may hang out of it now.
  const ids = new Set(siblings.map(itemId));
  return [...fitFrame(next, frame.id, bounds([r, ...next.filter((it) => ids.has(itemId(it))).map(rectOf)]) as Rect), placed];
}

/**
 * A new column in a frame: the first free place in its rows of columns, as many across as a tidy
 * would make for one more worktree, or else a new row under the rest.
 */
function newColumn(items: CanvasItem[], item: RefItem, frame: FrameItem): CanvasItem[] {
  const inside = childrenOf(items, frame.id).map(rectOf);
  const heads = items.filter((it) => it.kind === 'ref' && it.ref === 'worktree' && parentOf(items, it) === frame);
  const x0 = frame.x + FRAME_PAD;
  const step = CARD_W + COL_GAP;
  const rows = [...new Set((heads.length > 0 ? heads : inside).map((r) => r.y))].sort((a, b) => a - b);
  if (rows.length === 0) rows.push(frame.y + FRAME_PAD);
  // A frame of one row widens as a tidy would for one more worktree; once it has more rows, the
  // first sets how many go across, so the rows stay even.
  const fits = Math.floor((frame.w - FRAME_PAD * 2 + COL_GAP) / step);
  const across = rows.length > 1 ? Math.max(1, new Set(heads.filter((h) => h.y === rows[0]).map((h) => h.x)).size) : Math.max(fits, perRow(heads.length + 1));
  let spot: { x: number; y: number } | null = null;
  for (let i = 0; i < rows.length && !spot; i++) {
    const band = { y: rows[i], h: (rows[i + 1] ?? Infinity) - rows[i] - 1 };
    for (let j = 0; j < across; j++) {
      const slot = { x: x0 + j * step, w: CARD_W, ...band };
      if (!inside.some((r) => overlaps(r, slot, GAP - 1))) {
        spot = { x: slot.x, y: slot.y };
        break;
      }
    }
  }
  spot ??= { x: x0, y: Math.max(...inside.map((r) => r.y + r.h)) + COL_GAP * 2 };
  const placed = { ...item, ...spot };
  return [...fitFrame(items, frame.id, rectOf(placed)), placed];
}

/** The date in a tidy's day frame id (`auto:day:2026-8-26`), to find the latest. */
function frameDay(f: FrameItem): number | null {
  const m = /^auto:day:(\d+)-(\d+)-(\d+)$/.exec(f.id);
  return m ? new Date(+m[1], +m[2], +m[3]).getTime() : null;
}

/** The frame something with no worktree card to go under belongs in, by how the canvas was tidied. */
function frameFor(doc: CanvasDoc, e: Entities, item: RefItem): FrameItem | null {
  const frames = doc.items.filter((it): it is FrameItem => it.kind === 'frame');
  if (doc.tidy.layout === 'time') {
    const days = frames.filter((f) => frameDay(f) !== null).sort((a, b) => (frameDay(b) ?? 0) - (frameDay(a) ?? 0));
    if (days[0]) return days[0];
  }
  if (item.ref !== 'worktree') return frames.find((f) => f.id === 'auto:loose') ?? null;
  const repo = e.lanes.get(item.key)?.repo;
  const own = frames.find((f) => f.id === `auto:repo:${repo}`);
  if (own) return own;
  // Else the frame holding most of its repo's worktrees.
  const count = new Map<FrameItem, number>();
  for (const it of doc.items) {
    if (it.kind !== 'ref' || it.ref !== 'worktree' || e.lanes.get(it.key)?.repo !== repo) continue;
    const f = parentOf(doc.items, it);
    if (f) count.set(f, (count.get(f) ?? 0) + 1);
  }
  return [...count].sort((a, b) => b[1] - a[1])[0]?.[0] ?? null;
}

function placeNear(doc: CanvasDoc, e: Entities, item: RefItem): CanvasItem[] {
  const { ref, key } = item;
  const items = doc.items;
  const byId = new Map(items.map((it) => [itemId(it), it]));
  const lane = homeLane(e, ref, key);
  // A session, todo or PR goes under its worktree's card, below the lowest card of its column.
  const home = ref !== 'worktree' && lane ? byId.get(refId('worktree', lane)) : undefined;
  if (home) {
    const column = items.filter((it) => it.kind === 'ref' && Math.abs(it.x - home.x) < CARD_W / 2 && it.y >= home.y && homeLane(e, it.ref, it.key) === lane);
    return underColumn(items, item, [home, ...column.filter((it) => it !== home)]);
  }
  const frame = frameFor(doc, e, item);
  if (frame) {
    // Things with no worktree share a column per kind, as a tidy lays them out.
    if (ref !== 'worktree') {
      const kin = items.filter((it): it is RefItem => it.kind === 'ref' && it.ref === ref && parentOf(items, it) === frame && !byId.has(refId('worktree', homeLane(e, it.ref, it.key) ?? '')));
      const top = kin.sort((a, b) => a.y - b.y)[0];
      if (top) return underColumn(items, item, kin.filter((it) => Math.abs(it.x - top.x) < CARD_W / 2));
    }
    return newColumn(items, item, frame);
  }
  // Nowhere to belong: one column beside the top row of frames, or right of everything.
  const tops = items.filter((it) => it.kind === 'frame' && !parentOf(items, it)).map(rectOf);
  const all = bounds(tops.length > 0 ? tops : items.map(rectOf));
  const size = { w: CARD_W, h: CARD_H[ref] };
  const spot = all ? freeSpot(items, { ...size, x: all.x + all.w + FRAME_GAP, y: all.y }) : { x: 0, y: 0 };
  return [...items, { ...item, ...spot }];
}

/* ---- Tidy ---- */

export type TidyMode = 'worktree' | 'time';

export const TIDY_WORD: Record<TidyMode, string> = { worktree: 'Tidy by worktree', time: 'Tidy by day' };

const FRAME_COLORS: LabelColor[] = ['blue', 'teal', 'green', 'purple', 'indigo', 'pink', 'red'];

function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return h >>> 0;
}

/** A column: a worktree's card, then its sessions, then its todos and PRs. */
interface Column {
  lane: string | null;
  items: RefItem[];
  activity: number;
  /** Continues the column before it. */
  indent?: boolean;
}

/** A frame's worth of columns. */
interface Band {
  key: string;
  title: string;
  color: LabelColor;
  columns: Column[];
  activity: number;
  /** A frame `codealong board arrange` asked for, with its note: made `arranged`, not `auto`. */
  arranged?: { note: string | null };
}

function dayTitle(ts: number, now: number): string {
  const d = new Date(ts);
  d.setHours(0, 0, 0, 0);
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  const days = Math.round((today.getTime() - d.getTime()) / 86_400_000);
  if (days <= 0) return 'Today';
  if (days === 1) return 'Yesterday';
  if (days < 7) return d.toLocaleDateString('en-US', { weekday: 'long' });
  return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}

function dayKey(ts: number): string {
  const d = new Date(ts);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

export interface TidyPlan {
  /** Notes, frames you drew, and everything in those frames: left where they are. */
  stay: CanvasItem[];
  /** Cards to lay out. */
  move: RefItem[];
  /** Cards outside your frames older than the scope: they come off (and come back if they're active again). */
  dropped: RefItem[];
  /** Things within the scope that aren't on the canvas yet (and weren't taken off by hand). */
  added: RefItem[];
}

/** What a tidy with `opts` keeps, takes off and brings on, before it lays anything out. */
export function tidyPlan(doc: CanvasDoc, e: Entities, now: number, pinned: Set<string>, opts: TidyOptions, on: OnBoard = WHOLE): TidyPlan {
  const mine = doc.items.filter((it): it is FrameItem => it.kind === 'frame' && !it.auto);
  const inMine = (it: CanvasItem) => mine.some((f) => f !== it && contains(rectOf(f), rectOf(it)));
  const recent = recentRefs(e, now, pinned, opts, 0, on);
  const wanted = new Set(recent.map((r) => refId(r.ref, r.key)));
  const plan: TidyPlan = { stay: [], move: [], dropped: [], added: [] };
  for (const it of doc.items) {
    if (it.kind === 'frame') {
      if (!it.auto) plan.stay.push(it);
    } else if (it.kind === 'note' || (inMine(it) && !on.removed.has(itemId(it)))) plan.stay.push(it);
    // A PR you added stays as long as the board has it.
    else if (wanted.has(itemId(it)) || (it.ref === 'pr' && it.kept && e.prs.has(it.key))) plan.move.push(it);
    else plan.dropped.push(it);
  }
  const have = new Set(doc.items.map(itemId));
  for (const r of recent) {
    const id = refId(r.ref, r.key);
    if (have.has(id)) continue;
    const title = titleOf(e, r.ref, r.key) ?? undefined;
    plan.added.push({ kind: 'ref', ref: r.ref, key: r.key, x: 0, y: 0, ...(title ? { title } : {}) });
  }
  return plan;
}

/**
 * Lays the canvas out again, once, holding what `opts` says. Frames you drew stay where they are
 * with everything in them, and so do notes outside them; the frames the last tidy made are
 * replaced. Cards older than the options come off, whether or not you kept them, and cards within
 * them come on. Every card goes into a column with its worktree, and the columns into frames: one
 * per repo, or one per day of last activity. Frames are packed in rows, most recent first, below
 * what stayed. From then on Bring in adds only what moves on after it.
 */
export function tidy(doc: CanvasDoc, e: Entities, now: number, pinned: Set<string> = new Set(), opts: TidyOptions = doc.tidy, on: OnBoard = WHOLE): CanvasDoc {
  const plan = tidyPlan(doc, e, now, pinned, opts, on);
  const stay = plan.stay;
  const move = [...plan.move, ...plan.added];
  const { cleared: _, ...rest } = doc.scope;
  const scope: Scope = { ...rest, since: now };
  if (move.length === 0) return { ...doc, items: stay, scope, tidy: opts };
  const { frames, placed } = pack(tidyBands(move, e, opts.layout, now).map(layBand), stay);
  return { ...doc, items: [...frames, ...stay, ...placed], scope, tidy: opts };
}

const KIND_ORDER: Record<RefKind, number> = { worktree: 0, session: 1, todo: 2, pr: 3 };

/**
 * Cards in columns, one per worktree: its card, then its sessions, then its todos and PRs. Things
 * with none share a column per kind. Within a kind the most recent comes first, or with `rank`,
 * the lowest.
 */
function columnsOf(cards: RefItem[], e: Entities, rank?: (it: RefItem) => number): Column[] {
  const columns = new Map<string, Column>();
  for (const it of cards) {
    const lane = homeLane(e, it.ref, it.key);
    const key = lane ?? `loose:${it.ref}`;
    const col = columns.get(key) ?? { lane, items: [], activity: 0 };
    col.items.push(it);
    col.activity = Math.max(col.activity, lastActivityOf(e, it.ref, it.key));
    columns.set(key, col);
  }
  const within = rank ? (a: RefItem, b: RefItem) => rank(a) - rank(b) : (a: RefItem, b: RefItem) => lastActivityOf(e, b.ref, b.key) - lastActivityOf(e, a.ref, a.key) || a.key.localeCompare(b.key);
  for (const col of columns.values()) col.items.sort((a, b) => KIND_ORDER[a.ref] - KIND_ORDER[b.ref] || within(a, b));
  return [...columns.values()];
}

/** A long column continues in the next one over, under the same worktree card. */
function continued(columns: Column[]): Column[] {
  return columns.flatMap((col) => {
    const head = col.items[0]?.ref === 'worktree' ? [col.items[0]] : [];
    const rest = col.items.slice(head.length);
    if (rest.length <= COLUMN_MAX) return [col];
    const parts: Column[] = [];
    for (let i = 0; i < rest.length; i += COLUMN_MAX) parts.push({ ...col, items: [...(i === 0 ? head : []), ...rest.slice(i, i + COLUMN_MAX)], indent: i > 0 && head.length > 0 });
    return parts;
  });
}

/** The frames a tidy makes: a repo's worktrees, or a day's, the most recent first. */
function tidyBands(move: RefItem[], e: Entities, mode: TidyMode, now: number): Band[] {
  const bands = new Map<string, Band>();
  for (const col of columnsOf(move, e)) {
    const lane = col.lane ? e.lanes.get(col.lane) : undefined;
    let key: string;
    let title: string;
    if (mode === 'worktree') {
      key = lane ? `repo:${lane.repo}` : 'loose';
      title = lane ? lane.repo : 'Not in a worktree';
    } else {
      key = col.activity > 0 ? `day:${dayKey(col.activity)}` : 'loose';
      title = col.activity > 0 ? dayTitle(col.activity, now) : 'Gone';
    }
    const band = bands.get(key) ?? { key, title, color: mode === 'time' ? 'gray' : FRAME_COLORS[hash(title) % FRAME_COLORS.length], columns: [], activity: 0 };
    band.columns.push(col);
    band.activity = Math.max(band.activity, col.activity);
    bands.set(key, band);
  }
  const sorted = [...bands.values()].sort((a, b) => (a.key === 'loose' ? 1 : 0) - (b.key === 'loose' ? 1 : 0) || b.activity - a.activity || a.title.localeCompare(b.title));
  for (const b of sorted) b.columns = continued(b.columns.sort((x, y) => y.activity - x.activity || (x.lane ?? '').localeCompare(y.lane ?? '')));
  return sorted;
}

/** A band laid out in its frame: the frame's size, and where its cards (and note) go in it. */
interface Laid {
  band: Band;
  w: number;
  h: number;
  place: { it: RefItem; dx: number; dy: number }[];
  /** An arranged frame's note, in the frame. */
  note: Rect | null;
}

/** An arranged frame's note is at least two columns wide. */
const NOTE_W = CARD_W * 2 + COL_GAP;

/** How tall a note must be for its text: lines at the note's 14/20 type and padding
 *  (CanvasView.css), by an average character's width, and one to spare, since words wrap before
 *  the edge. */
function noteHeight(text: string, w: number): number {
  const perLine = Math.max(1, Math.floor((w - 28) / 7.2));
  const lines = text.split('\n').reduce((n, p) => n + Math.max(1, Math.ceil(p.length / perLine)), 0);
  return (lines + 1) * 20 + 24;
}

/** A band's columns, wrapping after a few so a repo with many worktrees makes a block, not a
 *  line; an arranged frame's note goes across the top. */
function layBand(band: Band): Laid {
  const across = perRow(band.columns.length);
  const place: Laid['place'] = [];
  let rowTop = 0;
  let w = 0;
  for (let i = 0; i < band.columns.length; i += across) {
    let rowH = 0;
    band.columns.slice(i, i + across).forEach((col, j) => {
      // A continued column starts level with the cards under the worktree, not with it.
      let y = col.indent ? CARD_H.worktree + GAP : 0;
      for (const it of col.items) {
        place.push({ it, dx: FRAME_PAD + j * (CARD_W + COL_GAP), dy: FRAME_PAD + rowTop + y });
        y += CARD_H[it.ref] + GAP;
      }
      rowH = Math.max(rowH, y - GAP);
      w = Math.max(w, (j + 1) * (CARD_W + COL_GAP) - COL_GAP);
    });
    rowTop += rowH + COL_GAP * 2;
  }
  let h = place.length > 0 ? rowTop - COL_GAP * 2 : 0;
  let note: Rect | null = null;
  const text = band.arranged?.note;
  if (text) {
    w = Math.max(w, NOTE_W);
    note = { x: FRAME_PAD, y: FRAME_PAD, w, h: noteHeight(text, w) };
    const down = note.h + (h > 0 ? COL_GAP : 0);
    for (const p of place) p.dy += down;
    h += down;
  }
  return { band, w: w + FRAME_PAD * 2, h: h + FRAME_PAD * 2, place, note };
}

/** Frames packed in rows, left to right, below what stayed: rows about as wide as a window's
 *  shape needs for the whole to fill it. */
function pack(laid: Laid[], stay: CanvasItem[]): { frames: FrameItem[]; placed: CanvasItem[] } {
  const kept = bounds(stay.map(rectOf));
  const x0 = kept ? kept.x : 0;
  let y = kept ? kept.y + kept.h + FRAME_GAP * 1.5 : 0;
  const area = laid.reduce((n, l) => n + (l.w + FRAME_GAP) * (l.h + FRAME_GAP), 0);
  const rowWidth = Math.max(2400, Math.sqrt(area * 1.7), ...laid.map((l) => l.w));
  const frames: FrameItem[] = [];
  const placed: CanvasItem[] = [];
  let x = x0;
  let rowH = 0;
  for (const l of laid) {
    if (x > x0 && x - x0 + l.w > rowWidth) {
      x = x0;
      y += rowH + FRAME_GAP;
      rowH = 0;
    }
    const { band } = l;
    const at = { x, y, w: l.w, h: l.h, title: band.title, color: band.color };
    frames.push(band.arranged ? { kind: 'frame', id: band.key, ...at, arranged: true } : { kind: 'frame', id: `auto:${band.key}`, ...at, auto: true });
    if (l.note && band.arranged?.note) placed.push({ kind: 'note', id: `${band.key}:note`, text: band.arranged.note, x: x + l.note.x, y: y + l.note.y, w: l.note.w, h: l.note.h, color: 'gray', arranged: true });
    for (const p of l.place) placed.push({ ...p.it, x: x + p.dx, y: y + p.dy });
    x += l.w + FRAME_GAP;
    rowH = Math.max(rowH, l.h);
  }
  return { frames, placed };
}

/* ---- Arranging by what the work is for ---- */

/** A frame `codealong board arrange` asks for: its name, color and note, and the cards that go
 *  in it, in order. */
export interface ArrangeFrame {
  title: string;
  color?: LabelColor;
  note?: string;
  cards: { ref: RefKind; key: string }[];
}

/** What becomes of the cards on the canvas no frame names: laid out after the frames as a tidy
 *  would lay them out, or taken off. */
export type ArrangeRest = 'keep' | 'off';

export interface Arranged {
  doc: CanvasDoc;
  /** Each frame made, with how many cards went in it. One with no cards and no note isn't made. */
  frames: { id: string; title: string; cards: number }[];
  /** Cards the board doesn't have in this canvas's project: left out. */
  missing: { ref: RefKind; key: string }[];
  /** How many cards on the canvas no frame named: laid out after the frames, or taken off. */
  rest: number;
}

/**
 * Lays the canvas out in the frames an agent asks for, by what the work is for: each frame's cards
 * in columns by worktree, in the order given, its note across its top, and the frames in rows in
 * the order given, below what stays. What stays is what a tidy leaves, frames you drew with what's
 * in them and notes, less the cards a frame names, which move to it. The frames and notes the last
 * arrange made are replaced; a tidy leaves them as frames you drew. A card named twice goes in the
 * first frame that names it. The cards no frame names go below the frames, laid out as a tidy
 * would, or with `rest` 'off' come off. Like a tidy, it sets where Bring in starts from, so it
 * first brings on what Bring in would have: a canvas read from its file, whose project no window
 * showed, may be behind the board.
 */
export function arrange(given: CanvasDoc, e: Entities, asked: ArrangeFrame[], rest: ArrangeRest, now: number, pinned: Set<string> = new Set(), holds: OnBoard = WHOLE): Arranged {
  const doc = sync(given, e, now, pinned, holds);
  const named = new Map<string, { ref: RefKind; key: string; frame: number; rank: number }>();
  const missing: Arranged['missing'] = [];
  asked.forEach((f, frame) => {
    for (const { ref, key } of f.cards) {
      const id = refId(ref, key);
      if (named.has(id) || missing.some((m) => refId(m.ref, m.key) === id)) continue;
      if (titleOf(e, ref, key) === null) missing.push({ ref, key });
      else named.set(id, { ref, key, frame, rank: named.size });
    }
  });

  const yours = doc.items.filter((it): it is FrameItem => it.kind === 'frame' && !it.auto && !it.arranged);
  const inYours = (it: CanvasItem) => yours.some((f) => contains(rectOf(f), rectOf(it)));
  const stay: CanvasItem[] = [];
  const others: RefItem[] = [];
  const on = new Map<string, RefItem>();
  for (const it of doc.items) {
    if (it.kind !== 'ref') {
      if (!(it.kind === 'frame' && it.auto) && !it.arranged) stay.push(it);
    } else if (named.has(itemId(it))) on.set(itemId(it), it);
    else if (inYours(it)) stay.push(it);
    else others.push(it);
  }

  const rank = (it: RefItem) => named.get(itemId(it))?.rank ?? Infinity;
  const first = (col: Column) => Math.min(...col.items.map(rank));
  const bands: Band[] = [];
  const made: Arranged['frames'] = [];
  asked.forEach((f, i) => {
    const cards = [...named.values()]
      .filter((n) => n.frame === i)
      .map((n): RefItem => {
        const title = titleOf(e, n.ref, n.key) ?? undefined;
        return { ...(on.get(refId(n.ref, n.key)) ?? { kind: 'ref', ref: n.ref, key: n.key, x: 0, y: 0 }), kept: true, ...(title ? { title } : {}) };
      });
    const note = f.note?.trim() || null;
    if (cards.length === 0 && !note) return;
    const title = f.title.trim() || 'Frame';
    const key = `arrange:${now}:${i}`;
    const color = f.color && LABEL_COLORS.includes(f.color) ? f.color : FRAME_COLORS[hash(title) % FRAME_COLORS.length];
    bands.push({ key, title, color, columns: continued(columnsOf(cards, e, rank).sort((a, b) => first(a) - first(b))), activity: 0, arranged: { note } });
    made.push({ id: key, title, cards: cards.length });
  });

  // The rest starts a row of its own under the frames asked for, so those read as one picture.
  const top = pack(bands.map(layBand), stay);
  const below = rest === 'keep' && others.length > 0 ? pack(tidyBands(others, e, doc.tidy.layout, now).map(layBand), [...stay, ...top.frames]) : { frames: [], placed: [] };
  const { cleared: _, ...scope } = doc.scope;
  return {
    doc: {
      ...doc,
      items: [...top.frames, ...below.frames, ...stay, ...top.placed, ...below.placed],
      scope: { ...scope, since: now },
    },
    frames: made,
    missing,
    rest: others.length,
  };
}

/** Who made a frame or a note: you, a tidy, or `codealong board arrange`. */
export type Maker = 'you' | 'tidy' | 'arrange';

/** What a canvas holds, as `codealong board show --as canvas` tells it: frames in reading order, then cards
 *  and notes by the frame each sits in (null for none) and where in it. */
export interface Outline {
  frames: { id: string; title: string; color: LabelColor; by: Maker; frame: string | null }[];
  cards: { ref: RefKind; key: string; title: string | null; frame: string | null; gone: boolean }[];
  notes: { id: string; text: string; frame: string | null; by: Maker }[];
}

export function outline(doc: CanvasDoc, e: Entities): Outline {
  const items = doc.items;
  const frames = items.filter((it): it is FrameItem => it.kind === 'frame').sort((a, b) => a.y - b.y || a.x - b.x);
  const place = new Map(frames.map((f, i) => [f.id, i]));
  const frameOf = (it: CanvasItem) => parentOf(items, it)?.id ?? null;
  // In a frame's order, then a column at a time, top to bottom; what's in no frame last.
  const inOrder = <T extends CanvasItem>(list: T[]) =>
    list
      .map((it) => ({ it, frame: frameOf(it) }))
      .sort((a, b) => (a.frame === null ? frames.length : place.get(a.frame)!) - (b.frame === null ? frames.length : place.get(b.frame)!) || a.it.x - b.it.x || a.it.y - b.it.y);
  return {
    frames: frames.map((f) => ({ id: f.id, title: f.title, color: f.color, by: f.auto ? 'tidy' : f.arranged ? 'arrange' : 'you', frame: frameOf(f) })),
    cards: inOrder(items.filter((it): it is RefItem => it.kind === 'ref')).map(({ it, frame }) => {
      const title = titleOf(e, it.ref, it.key);
      return { ref: it.ref, key: it.key, title: title ?? it.title ?? null, frame, gone: title === null };
    }),
    notes: inOrder(items.filter((it): it is NoteItem => it.kind === 'note')).map(({ it, frame }) => ({ id: it.id, text: it.text, frame, by: it.arranged ? 'arrange' : 'you' })),
  };
}

/* ---- Links between cards ---- */

/** Pairs of canvas ids that belong together: a session and its worktree, a todo and what it's linked to or waits on, a PR and its worktree. */
export function links(e: Entities, on: Set<string>): [string, string][] {
  const out: [string, string][] = [];
  const add = (a: string, b: string) => {
    if (on.has(a) && on.has(b)) out.push([a, b]);
  };
  for (const { session, lane } of e.sessions.values()) add(refId('session', session.key), refId('worktree', lane.id));
  for (const t of e.todos.values()) {
    const id = refId('todo', t.id);
    for (const w of t.worktrees) add(id, refId('worktree', w));
    for (const s of t.sessions) add(id, refId('session', s));
    for (const d of t.dependsOn ?? []) add(id, refId('todo', d));
  }
  for (const p of e.prs.values()) for (const l of p.lanes) add(refId('pr', p.key), refId('worktree', l));
  return out;
}

/** Pairs of sessions on the canvas where one came from the other (`Session.origin`): the one it
 *  came from first. */
export function originLinks(e: Entities, on: Set<string>): [string, string][] {
  const out: [string, string][] = [];
  for (const { session } of e.sessions.values()) {
    if (!session.origin) continue;
    const from = refId('session', session.origin.key);
    const to = refId('session', session.key);
    if (from !== to && on.has(from) && on.has(to)) out.push([from, to]);
  }
  return out;
}
