// The Canvas view's document: where you put each worktree, session, todo and PR, and your notes
// and frames. One per project, kept by the engine in canvas.json (canvas.rs).
//
// The canvas is for remembering where work is, so positions hold still: nothing moves unless you
// move it or ask for a tidy. New work lands in free space beside what it belongs to (a session
// under its worktree, a todo beside its worktree) without pushing anything aside. What shows by
// default is recent work; the rest comes in when you add it, and what you've moved or added
// stays, drawn as gone once the board no longer has it.

import { sessionHomes, type Placed } from '../todos';
import type { Board, Lane, LabelColor, PrItem, Session, Todo } from '../types';

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
  /** Ids of recent things you took off the canvas, so they don't come back by themselves. */
  hidden: string[];
  camera: Camera | null;
  /** What the canvas holds by itself, and how the last tidy laid it out. */
  scope: Scope;
}

/** How far back finished sessions go on the canvas. Busy ones are always on it. */
export type SessionAge = '1d' | '3d' | '7d' | 'all';
/** Which todos are on the canvas: open ones changed in the last week or month, every open one, or none. */
export type TodoAge = '7d' | '30d' | 'open' | 'none';

export interface Scope {
  sessions: SessionAge;
  todos: TodoAge;
  layout: TidyMode;
}

export const DEFAULT_SCOPE: Scope = { sessions: '1d', todos: 'open', layout: 'worktree' };

export const SESSION_AGES: { id: SessionAge; label: string; title: string }[] = [
  { id: '1d', label: '24h', title: 'Active in the last 24 hours' },
  { id: '3d', label: '3d', title: 'Active in the last 3 days' },
  { id: '7d', label: '7d', title: 'Active in the last 7 days' },
  { id: 'all', label: 'All', title: 'Every session on the board: the last 14 days' },
];

export const TODO_AGES: { id: TodoAge; label: string; title: string }[] = [
  { id: '7d', label: '7d', title: 'Open todos changed or worked on in the last 7 days' },
  { id: '30d', label: '30d', title: 'Open todos changed or worked on in the last 30 days' },
  { id: 'open', label: 'Open', title: 'Every open todo' },
  { id: 'none', label: 'None', title: 'No todos' },
];

const DAY_MS = 24 * 60 * 60 * 1000;
const SESSION_MS: Record<SessionAge, number> = { '1d': DAY_MS, '3d': 3 * DAY_MS, '7d': 7 * DAY_MS, all: Infinity };
const TODO_MS: Record<TodoAge, number> = { '7d': 7 * DAY_MS, '30d': 30 * DAY_MS, open: Infinity, none: -1 };

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
  return { version: 1, items: [], hidden: [], camera: null, scope: DEFAULT_SCOPE };
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
  if (Array.isArray(r.hidden)) doc.hidden = r.hidden.filter((h): h is string => typeof h === 'string');
  const sc = r.scope as Partial<Scope> | undefined;
  if (sc && typeof sc === 'object') {
    doc.scope = {
      sessions: SESSION_AGES.some((a) => a.id === sc.sessions) ? (sc.sessions as SessionAge) : DEFAULT_SCOPE.sessions,
      todos: TODO_AGES.some((a) => a.id === sc.todos) ? (sc.todos as TodoAge) : DEFAULT_SCOPE.todos,
      layout: sc.layout === 'time' ? 'time' : 'worktree',
    };
  }
  if (r.camera && num(r.camera.x) && num(r.camera.y) && num(r.camera.zoom) && r.camera.zoom > 0) doc.camera = r.camera;
  return doc;
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

/** What the canvas can show, from the board, for one project (or all of them). */
export interface Entities {
  lanes: Map<string, Lane>;
  sessions: Map<string, Placed>;
  todos: Map<string, Todo>;
  prs: Map<string, PrItem>;
}

/** `lanes` are the project's, as the window shows them; todos and PRs are picked by `inProject`. */
export function entitiesOf(board: Board | null, lanes: Lane[], todos: Todo[], inProject: (project: string) => boolean): Entities {
  const sessions = new Map([...sessionHomes(lanes)].filter(([, p]) => !p.session.archived));
  return {
    lanes: new Map(lanes.map((l) => [l.id, l])),
    sessions,
    todos: new Map(todos.filter((t) => !t.project || inProject(t.project)).map((t) => [t.id, t])),
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
 * What the canvas holds by itself, in the order it's placed (worktrees first, so the rest can
 * land beside them): sessions active within the scope's age, and any busy one (working, or
 * waiting on you); open todos within theirs, counting their sessions' activity; worktrees that
 * are pinned, running something, active within the sessions' age, or home to a session or todo
 * on the canvas. PRs show on their worktree's card, so they come on only when you add them.
 */
export function recentRefs(e: Entities, now: number, pinned: Set<string> = new Set(), scope: Scope = DEFAULT_SCOPE): { ref: RefKind; key: string }[] {
  const since = now - SESSION_MS[scope.sessions];
  const sessions = [...e.sessions.values()].filter(({ session: s }) => sessionIn(s, since));
  sessions.sort((a, b) => b.session.lastPromptAt - a.session.lastPromptAt);
  const todos = [...e.todos.values()].filter((t) => todoIn(e, t, now, scope.todos));
  const homes = new Set<string>();
  for (const s of sessions) homes.add(s.lane.id);
  for (const t of todos) {
    const home = homeLane(e, 'todo', t.id);
    if (home) homes.add(home);
  }
  const lanes = [...e.lanes.values()].filter((l) => pinned.has(l.id) || homes.has(l.id) || l.lastActivity >= since || l.sessions.some((s) => s.live && !s.visitor));
  lanes.sort((a, b) => a.repo.localeCompare(b.repo) || b.lastActivity - a.lastActivity);
  return [
    ...lanes.map((l) => ({ ref: 'worktree' as const, key: l.id })),
    ...sessions.map((s) => ({ ref: 'session' as const, key: s.session.key })),
    ...todos.map((t) => ({ ref: 'todo' as const, key: t.id })),
  ];
}

/** A finished session that was never archived is still "unread" or idle; only busy ones outlast the age. */
function sessionIn(s: Session, since: number): boolean {
  return s.live || s.status === 'working' || s.status === 'needs' || s.status === 'error' || s.lastActivity >= since;
}

function todoIn(e: Entities, t: Todo, now: number, age: TodoAge): boolean {
  if (t.status === 'done' || age === 'none') return false;
  if (age === 'open') return true;
  const worked = Math.max(t.updatedAt, ...t.sessions.map((k) => e.sessions.get(k)?.session.lastActivity ?? 0));
  return worked >= now - TODO_MS[age];
}

/**
 * Brings the document up to date with the board: recent things not on it yet land beside what
 * they belong to, recent things you never touched leave once they're no longer recent, and kept
 * things note their current title. Returns the same document when nothing changed.
 */
export function sync(doc: CanvasDoc, e: Entities, now: number, pinned: Set<string> = new Set()): CanvasDoc {
  let changed = false;
  const recent = recentRefs(e, now, pinned, doc.scope);
  const wanted = new Set(recent.map((r) => refId(r.ref, r.key)));
  const items = doc.items.filter((it) => {
    if (it.kind !== 'ref' || it.kept || wanted.has(itemId(it))) return true;
    changed = true;
    return false;
  });
  for (let i = 0; i < items.length; i++) {
    const it = items[i];
    if (it.kind !== 'ref' || !it.kept) continue;
    const title = titleOf(e, it.ref, it.key);
    if (title && title !== it.title) {
      items[i] = { ...it, title };
      changed = true;
    }
  }
  const have = new Set(items.map(itemId));
  const hidden = new Set(doc.hidden);
  let next: CanvasDoc = changed ? { ...doc, items } : doc;
  for (const r of recent) {
    const id = refId(r.ref, r.key);
    if (have.has(id) || hidden.has(id)) continue;
    next = addRef(next, e, r.ref, r.key, null, false);
    have.add(id);
  }
  return next;
}

/**
 * Puts a thing on the canvas: at `at` when given (a drop, a right-click), else in free space
 * beside what it belongs with, else to the right of everything. Taking it off the hidden list
 * too, and keeping it when `kept`.
 */
export function addRef(doc: CanvasDoc, e: Entities, ref: RefKind, key: string, at: { x: number; y: number } | null, kept: boolean): CanvasDoc {
  const id = refId(ref, key);
  const existing = doc.items.find((it) => itemId(it) === id);
  if (existing) return doc;
  const size = { w: CARD_W, h: CARD_H[ref] };
  const spot = at ? freeSpot(doc.items, { ...size, x: at.x, y: at.y }) : placeNear(doc, e, ref, key, size);
  const title = titleOf(e, ref, key) ?? undefined;
  const item: RefItem = { kind: 'ref', ref, key, x: spot.x, y: spot.y, ...(kept ? { kept: true } : {}), ...(title ? { title } : {}) };
  return { ...doc, items: [...doc.items, item], hidden: doc.hidden.filter((h) => h !== id) };
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

function placeNear(doc: CanvasDoc, e: Entities, ref: RefKind, key: string, size: { w: number; h: number }): { x: number; y: number } {
  const byId = new Map(doc.items.map((it) => [itemId(it), it]));
  const lane = homeLane(e, ref, key);
  // A session or todo goes under its worktree's card, or under the lowest card of its column.
  if (ref !== 'worktree' && lane) {
    const home = byId.get(refId('worktree', lane));
    if (home) {
      const column = doc.items.filter((it) => it.kind === 'ref' && Math.abs(it.x - home.x) < CARD_W / 2 && it.y >= home.y && homeLane(e, it.ref, it.key) === lane);
      const bottom = Math.max(...column.map((it) => rectOf(it).y + rectOf(it).h));
      return freeSpot(doc.items, { ...size, x: home.x, y: bottom + GAP });
    }
  }
  // A worktree goes to the right of the other worktrees of its repo.
  if (ref === 'worktree') {
    const repo = e.lanes.get(key)?.repo;
    const kin = doc.items.filter((it): it is RefItem => it.kind === 'ref' && it.ref === 'worktree' && e.lanes.get(it.key)?.repo === repo);
    if (kin.length > 0) {
      const top = Math.min(...kin.map((k) => k.y));
      const right = Math.max(...kin.map((k) => k.x + CARD_W));
      return freeSpot(doc.items, { ...size, x: right + COL_GAP, y: top });
    }
  }
  const all = bounds(doc.items.map(rectOf));
  if (!all) return { x: 0, y: 0 };
  return freeSpot(doc.items, { ...size, x: all.x + all.w + FRAME_GAP, y: all.y });
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

interface Band {
  key: string;
  title: string;
  color: LabelColor;
  columns: Column[];
  activity: number;
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

/** What a tidy with `scope` keeps, takes off and brings on, before it lays anything out. */
export function tidyPlan(doc: CanvasDoc, e: Entities, now: number, pinned: Set<string>, scope: Scope): TidyPlan {
  const mine = doc.items.filter((it): it is FrameItem => it.kind === 'frame' && !it.auto);
  const inMine = (it: CanvasItem) => mine.some((f) => f !== it && contains(rectOf(f), rectOf(it)));
  const recent = recentRefs(e, now, pinned, scope);
  const wanted = new Set(recent.map((r) => refId(r.ref, r.key)));
  const plan: TidyPlan = { stay: [], move: [], dropped: [], added: [] };
  for (const it of doc.items) {
    if (it.kind === 'frame') {
      if (!it.auto) plan.stay.push(it);
    } else if (it.kind === 'note' || inMine(it)) plan.stay.push(it);
    // A PR is on the canvas only because you put it there.
    else if (wanted.has(itemId(it)) || (it.ref === 'pr' && e.prs.has(it.key))) plan.move.push(it);
    else plan.dropped.push(it);
  }
  const have = new Set(doc.items.map(itemId));
  const hidden = new Set(doc.hidden);
  for (const r of recent) {
    const id = refId(r.ref, r.key);
    if (have.has(id) || hidden.has(id)) continue;
    const title = titleOf(e, r.ref, r.key) ?? undefined;
    plan.added.push({ kind: 'ref', ref: r.ref, key: r.key, x: 0, y: 0, ...(title ? { title } : {}) });
  }
  return plan;
}

/**
 * Lays the canvas out again, holding what `scope` says. Frames you drew stay where they are with
 * everything in them, and so do notes outside them; the frames the last tidy made are replaced.
 * Cards older than the scope come off, whether or not you kept them, and cards within it come
 * on. Every card goes into a column with its worktree, and the columns into frames: one per
 * repo, or one per day of last activity. Frames are packed in rows, most recent first, below
 * what stayed.
 */
export function tidy(doc: CanvasDoc, e: Entities, now: number, pinned: Set<string> = new Set(), scope: Scope = doc.scope): CanvasDoc {
  const mode = scope.layout;
  const plan = tidyPlan(doc, e, now, pinned, scope);
  const stay = plan.stay;
  const move = [...plan.move, ...plan.added];
  if (move.length === 0) return { ...doc, items: stay, scope };

  // Columns, one per worktree; things with none share a column per kind.
  const columns = new Map<string, Column>();
  for (const it of move) {
    const lane = homeLane(e, it.ref, it.key);
    const key = lane ?? `loose:${it.ref}`;
    const col = columns.get(key) ?? { lane, items: [], activity: 0 };
    col.items.push(it);
    col.activity = Math.max(col.activity, lastActivityOf(e, it.ref, it.key));
    columns.set(key, col);
  }
  const order: Record<RefKind, number> = { worktree: 0, session: 1, todo: 2, pr: 3 };
  for (const col of columns.values()) {
    col.items.sort((a, b) => order[a.ref] - order[b.ref] || lastActivityOf(e, b.ref, b.key) - lastActivityOf(e, a.ref, a.key) || a.key.localeCompare(b.key));
  }

  // Bands: a repo's worktrees, or a day's.
  const bands = new Map<string, Band>();
  for (const col of columns.values()) {
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
  const sortedBands = [...bands.values()].sort((a, b) => (a.key === 'loose' ? 1 : 0) - (b.key === 'loose' ? 1 : 0) || b.activity - a.activity || a.title.localeCompare(b.title));
  for (const b of sortedBands) b.columns.sort((x, y) => y.activity - x.activity || (x.lane ?? '').localeCompare(y.lane ?? ''));

  // A long column continues in the next one over, under the same worktree card.
  for (const band of bands.values()) {
    band.columns = band.columns.flatMap((col) => {
      const head = col.items[0]?.ref === 'worktree' ? [col.items[0]] : [];
      const rest = col.items.slice(head.length);
      if (rest.length <= COLUMN_MAX) return [col];
      const parts: Column[] = [];
      for (let i = 0; i < rest.length; i += COLUMN_MAX) parts.push({ ...col, items: [...(i === 0 ? head : []), ...rest.slice(i, i + COLUMN_MAX)], indent: i > 0 && head.length > 0 });
      return parts;
    });
  }

  // Each band's columns wrap after a few, so a repo with many worktrees makes a block, not a line.
  const perRow = (n: number) => Math.max(1, Math.min(8, Math.ceil(Math.sqrt(n * 2.5))));
  const laid: { band: Band; w: number; h: number; place: { it: RefItem; dx: number; dy: number }[] }[] = [];
  for (const band of sortedBands) {
    const across = perRow(band.columns.length);
    const place: { it: RefItem; dx: number; dy: number }[] = [];
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
    laid.push({ band, w: w + FRAME_PAD * 2, h: rowTop - COL_GAP * 2 + FRAME_PAD * 2, place });
  }

  // Frames packed in rows, left to right, below what stayed.
  const kept = bounds(stay.map(rectOf));
  const x0 = kept ? kept.x : 0;
  let y = kept ? kept.y + kept.h + FRAME_GAP * 1.5 : 0;
  // Rows about as wide as a window's shape needs for the whole to fill it.
  const area = laid.reduce((n, l) => n + (l.w + FRAME_GAP) * (l.h + FRAME_GAP), 0);
  const rowWidth = Math.max(2400, Math.sqrt(area * 1.7), ...laid.map((l) => l.w));
  const frames: FrameItem[] = [];
  const cards: RefItem[] = [];
  let x = x0;
  let rowH = 0;
  for (const l of laid) {
    if (x > x0 && x - x0 + l.w > rowWidth) {
      x = x0;
      y += rowH + FRAME_GAP;
      rowH = 0;
    }
    frames.push({ kind: 'frame', id: `auto:${l.band.key}`, title: l.band.title, x, y, w: l.w, h: l.h, color: l.band.color, auto: true });
    for (const p of l.place) cards.push({ ...p.it, x: x + p.dx, y: y + p.dy });
    x += l.w + FRAME_GAP;
    rowH = Math.max(rowH, l.h);
  }
  return { ...doc, items: [...frames, ...stay, ...cards], scope };
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
