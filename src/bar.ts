// How the floating bar lays out what it shows (the engine picks and orders it: barfeed.rs):
// when the pills don't all fit, the least urgent group folds its last pills into one that lists
// them: Working first, then Your turn, then Needs you. Also what its project menu counts.

import { withTimes } from './format';
import { ALL_PROJECTS, inProject } from './quick';
import { countedKinds, inOrder, inQueueOrder, statusWaits, todoRowKey, WAITS, type Reads, type Waits } from './queue';
import type { BarGroup, BarItem, BarReview, BarTodo } from './types';

export type Group = BarGroup;
/** Bar order, most urgent first. */
export const GROUPS: Group[] = ['needs', 'unread', 'working'];
/** How long a click on the bar, a desktop pin or a PR's chip waits for a second one: one click
 *  stays in Codealong, two open the session where it lives or the PR on GitHub, without the first
 *  click's doing showing first. */
export const CLICK_WAIT_MS = 250;
/** Where the project picked in the bar's menu is kept: the peek reads it too, to say a session's
 *  project as the bar would (ui/ProjectSaid.tsx). */
export const BAR_PROJECT_KEY = 'codealong.bar.project';
/** The widest the floating bar gets before its pills fold, and the room it leaves at the screen's
 *  sides. Wide enough for at least one Needs you pill beside the project button and every summary
 *  pill (snoozed, recent, reviews, merges, due, ready). */
export const FLOATING_MAX_WIDTH = 1100;
export const FLOATING_SCREEN_ROOM = 48;

/** The room the floating bar fits its pills into on a screen `availWidth` points wide. */
export function floatingRoom(availWidth: number): number {
  return Math.min(FLOATING_MAX_WIDTH, availWidth - FLOATING_SCREEN_ROOM);
}

/** The order groups fold in when the bar runs out of room. */
const FOLD_ORDER: Group[] = ['working', 'unread', 'needs'];

/** A group on the bar: its first `shown` pills, then one pill holding the other `folded`. */
export interface GroupFit {
  group: Group;
  shown: number;
  folded: number;
}

/** Sizes that decide how much fits, in CSS pixels, as the bar's styles lay it out. */
export interface Metrics {
  /** Each pill's own width, by group, in bar order. */
  pills: Record<Group, number[]>;
  /** A pill holding folded ones, per group. */
  fold: Record<Group, number>;
  /** The bar's mark, and the project menu's button beside it (0 for one that isn't among the
   *  pills: docked in the notch, both are beside the camera). */
  mark: number;
  project: number;
  /** The pills of the sessions that are hearing, each its own width, first after those. */
  hearing: number[];
  /** The snoozed, recent, reviews, merges, due and todos pills at the end, side by side (0 for one
   *  that isn't there). */
  snoozed: number;
  recent: number;
  reviews: number;
  merges: number;
  due: number;
  todos: number;
  /** A note on the bar (a failed open, what a review did), 0 while there's none. It's counted in
   *  a section of its own at the end, the most it takes, whether it shows there or in the place
   *  of the pill it came from. */
  note: number;
  /** Space between neighbors, a divider's own width, and the bar's padding on each side. */
  gap: number;
  divider: number;
  padding: number;
}

/** The bar's width with the groups laid out as `fits` say. Sections sit apart by dividers. */
export function barWidth(fits: GroupFit[], m: Metrics): number {
  const sections: number[][] = [[m.mark, m.project].filter((w) => w > 0), m.hearing];
  for (const f of fits) sections.push([...m.pills[f.group].slice(0, f.shown), ...(f.folded > 0 ? [m.fold[f.group]] : [])]);
  sections.push([m.snoozed, m.recent, m.reviews, m.merges, m.due, m.todos].filter((w) => w > 0), m.note > 0 ? [m.note] : []);
  const shown = sections.filter((s) => s.length > 0);
  if (!shown.length) return 2 * m.padding;
  const pieces = shown.flat();
  const dividers = shown.length - 1;
  const children = pieces.length + dividers;
  return 2 * m.padding + pieces.reduce((a, b) => a + b, 0) + dividers * m.divider + (children - 1) * m.gap;
}

/**
 * Folds pills until the bar fits in `room`: Working's from its last pill back, then Your turn's,
 * then Needs you's. A group with every pill folded shows just the one pill with its count.
 */
export function fitGroups(m: Metrics, room: number): GroupFit[] {
  const fits = GROUPS.map((group) => ({ group, shown: m.pills[group].length, folded: 0 }));
  for (const group of FOLD_ORDER) {
    const f = fits.find((x) => x.group === group)!;
    const n = m.pills[group].length;
    while (barWidth(fits, m) > room && f.shown > 0) {
      f.shown -= 1;
      f.folded = n - f.shown;
    }
    if (barWidth(fits, m) <= room) break;
  }
  return fits;
}

/** Text with its backtick code spans and time tokens made plain, for tooltips and menus. */
export function plain(text: string, now: number): string {
  return withTimes(text.replace(/`/g, ''), now);
}

/** What waits in a project: sessions that need you, and turns that ended. */
export interface WaitCounts {
  needs: number;
  unread: number;
}

/** Needs you and Your turn in each project and in all of them, for the project menu, the way the
 *  quick panel counts them. */
export function projectWaits(items: BarItem[], projects: string[]): Record<string, WaitCounts> {
  const out: Record<string, WaitCounts> = {};
  for (const name of [...projects, ALL_PROJECTS]) {
    const c = { needs: 0, unread: 0 };
    for (const i of items) if (i.group !== 'working' && inProject(i, name)) c[i.group] += 1;
    out[name] = c;
  }
  return out;
}

/** What waits in the projects the bar doesn't show, for the badge on its project button: nothing
 *  while it shows them all. */
export function elsewhereWaits(waits: Record<string, WaitCounts>, projects: string[], current: string): WaitCounts {
  const out = { needs: 0, unread: 0 };
  if (current === ALL_PROJECTS) return out;
  for (const name of projects) {
    if (name === current) continue;
    out.needs += waits[name]?.needs ?? 0;
    out.unread += waits[name]?.unread ?? 0;
  }
  return out;
}

// ---- The bar docked in the notch (Notch.tsx) ----

/** The docked bar's measures, in points: each side beside the camera while it's closed, and how
 *  much wider it grows while the pointer is on it; the concave corners where it meets the menu
 *  bar and its bottom corners, closed, with the pills showing and with a session under them; the
 *  padding around the pills; the session under them, `width` by `height` until you drag it to a
 *  size of your own, never narrower than `width` or shorter than `least`, nor wider than `widest`
 *  or taller than `tallest`, and leaving `clear` of the screen beside and under it; and the room
 *  the window keeps beside and under the shape for its shadow (app.css `--shadow-notch` reaches
 *  16 beside it and 30 under it). */
export const NOTCH = {
  wing: 44,
  hover: 4,
  ear: { closed: 6, open: 14 },
  radius: { closed: 12, open: 22, session: 26 },
  pad: 10,
  session: { width: 680, height: 520, least: 240, widest: 1040, tallest: 900, clear: 96 },
  shadow: { side: 20, bottom: 32 },
};

export interface Size {
  w: number;
  h: number;
}

/** What the docked bar shows: the notch alone; the notch a little wider, with the pointer just
 *  on it; the bar's pills under it, while the pointer stays or it's pinned; a session under the
 *  pills, to read and answer; or the end of what waits under them (QueueEnd.tsx). */
export type NotchState = 'closed' | 'hover' | 'pills' | 'session' | 'end';

export interface NotchSizes {
  closed: Size;
  pills: Size;
  session: Size;
  /** With the end of what waits under the pills: as tall as it needs, whatever size you dragged
   *  the session to. */
  end: Size;
  /** The biggest the shape can get, a session dragged as far as it goes: what everything is laid
   *  out in. */
  full: Size;
  window: Size;
}

/** The docked bar's shape in each state, and its window. Open, it's one width whatever it shows
 *  (the pills, a session under them, the end of what waits): the session's, at the width you
 *  dragged it to (`want`: its width, and its height under the pills) or its usual one, within
 *  what the screen has room for, and never narrower than the sides beside the camera need. The
 *  pills fold to fit it (Bar.tsx) rather than widen it: a shape that grew and shrank with each
 *  pill that came and went, a note's included, moved what you were reading (asked 2026-11-13).
 *  `pillsH` is how tall the pills are; a session under them is as much taller as it is.
 *  Everything is laid out once in the biggest it can get (`full`) and shown through a clip
 *  (`notchClip`). The window is that big too, with room for the corners and the shadow, so it
 *  keeps its size whatever shows, a session being dragged bigger included, and never cuts the
 *  shadow off. At a `zoom` (a factor) the session's card draws bigger (zoom.ts), so its usual
 *  size is as much bigger, within the screen; a size you dragged it to stays as you left it. The
 *  most it can get grows only for a usual size past it, so most zooms leave the window as it
 *  was: a window resized by the engine lands a moment after the page has laid out for it, and
 *  the shape jumped across in between. `endH` is how tall the end of what waits is at its width:
 *  it's as tall as that, a session dragged shorter or not, within the most the layout has.
 *  `soon` is how much wider each side is closed, for the countdown to a meeting beside the
 *  camera (Meeting.tsx `MeetingSoon`): the shape stays centered on it. */
export function notchSizes(notch: { width: number; height: number }, pillsH: number, side: number, screen: Size, want?: Size | null, zoom = 1, endH = 0, soon = 0): NotchSizes {
  const closed = { w: notch.width + 2 * (NOTCH.wing + Math.ceil(soon)), h: notch.height };
  // As much wider than closed on each side, in whole points: the clip's edges then fall on whole
  // points, where they're sharp.
  const widened = (w: number) => closed.w + 2 * Math.ceil(Math.max(0, w - closed.w) / 2);
  const top = Math.ceil(notch.height + pillsH + NOTCH.pad);
  const across = screen.w - NOTCH.session.clear;
  const roomDown = Math.floor(screen.h - top - NOTCH.session.clear);
  const usual = { w: Math.min(Math.round(NOTCH.session.width * zoom), across), h: Math.min(Math.round(NOTCH.session.height * zoom), roomDown) };
  const narrowest = Math.max(widened(usual.w), widened(notch.width + 2 * (side + NOTCH.pad)));
  const widest = Math.max(narrowest, widened(Math.min(NOTCH.session.widest, across)));
  const w = Math.min(widest, Math.max(narrowest, widened(want?.w ?? usual.w)));
  const tallest = Math.max(NOTCH.session.least, usual.h, Math.min(NOTCH.session.tallest, roomDown));
  const tall = Math.min(tallest, Math.max(NOTCH.session.least, Math.round(want?.h ?? usual.h)));
  const full = { w: widest, h: top + tallest };
  const margin = Math.max(NOTCH.ear.open, NOTCH.shadow.side);
  return {
    closed,
    pills: { w, h: top },
    session: { w, h: top + tall },
    end: { w, h: Math.min(full.h, top + Math.ceil(endH)) },
    full,
    window: { w: full.w + 2 * margin, h: full.h + NOTCH.shadow.bottom },
  };
}

/** The room the pills have across the open notch, which the bar folds them to fit. */
export function notchPillsRoom(s: Pick<NotchSizes, 'pills'>): number {
  return s.pills.w - 2 * NOTCH.pad;
}

/** How much of the layout shows: its insets from the sides and the bottom, and the radius of its
 *  bottom corners. Closed it's the notch and its sides, a little wider under the pointer. */
export function notchClip(s: Pick<NotchSizes, 'closed' | 'pills' | 'session' | 'end' | 'full'>, state: NotchState): { side: number; bottom: number; radius: number } {
  if (state === 'session') return { side: (s.full.w - s.session.w) / 2, bottom: s.full.h - s.session.h, radius: NOTCH.radius.session };
  if (state === 'end') return { side: (s.full.w - s.end.w) / 2, bottom: s.full.h - s.end.h, radius: NOTCH.radius.session };
  if (state === 'pills') return { side: (s.full.w - s.pills.w) / 2, bottom: s.full.h - s.pills.h, radius: NOTCH.radius.open };
  const side = Math.max(0, (s.full.w - s.closed.w) / 2 - (state === 'hover' ? NOTCH.hover : 0));
  return { side, bottom: s.full.h - s.closed.h, radius: NOTCH.radius.closed };
}

/** The edges that size the session in the notch: its bottom corners and its bottom edge. */
export type NotchEdge = 'left' | 'right' | 'bottom';

/** The size a session in the notch is dragged to, from the size it had (`from`: its width, and
 *  its height under the pills) by the pointer's move since the press. The notch stays centered,
 *  so a corner pulled out widens it as much on the other side. `notchSizes` keeps it within
 *  what there's room for. */
export function notchDragged(from: Size, edge: NotchEdge, dx: number, dy: number): Size {
  const out = edge === 'right' ? dx : edge === 'left' ? -dx : 0;
  return { w: from.w + 2 * out, h: from.h + dy };
}

/** What the notch counts beside the camera: the bar's groups, the PRs waiting on your review, the
 *  PRs of yours ready to merge, and the todos due. */
export type NotchKind = Group | 'reviews' | 'merges' | 'due';
/** What the notch counts, in the order the queue works through what waits (queue.ts
 *  `countedKinds`: your order, without the todos ready to start), then what runs. */
export function notchOrder(order: readonly Waits[] = WAITS): NotchKind[] {
  return [...(countedKinds(order) as NotchKind[]), 'working'];
}

/** What the closed notch shows beside the camera: everything that waits on you (the sessions that
 *  need you, the PRs waiting on your review, the PRs ready to merge, the todos due and Your turn,
 *  which the queue works through), drawn as the first kind among it in the queue's order
 *  (`notchOrder`); with nothing waiting, what's working; none when nothing runs either. The first
 *  kind's count alone read as all there was: 5 need you, with 16 more Your turn unsaid (asked
 *  2026-11-11). Open, each kind shows its own count. */
export function notchCount(counts: Record<NotchKind, number>, order: readonly Waits[] = WAITS): { kind: NotchKind; n: number } | null {
  const kinds = notchOrder(order);
  const kind = kinds.find((k) => counts[k] > 0);
  if (!kind) return null;
  return { kind, n: kind === 'working' ? counts.working : kinds.reduce((sum, k) => sum + (k === 'working' ? 0 : counts[k]), 0) };
}

// ---- What the notch works through (NotchSession.tsx, NotchReview.tsx) ----

/** A row of what the notch shows under its pills: a session on the bar, a PR that waits on your
 *  review or merge, or a todo due or ready to start (`waits`). Keyed by the session's key, the
 *  PR's (`owner/name#12`) or the todo's id after `todo:`, which never collide. */
export type NotchRow =
  | { kind: 'session'; key: string; item: BarItem }
  | { kind: 'review'; key: string; review: BarReview }
  | { kind: 'todo'; key: string; todo: BarTodo; waits: 'due' | 'ready' };

export { todoRowKey };

/** How the queue (queue.ts) reads the notch's rows: a pill carries its session's key and times
 *  without the session, a review or merge the PR's. */
export const ROW_READS: Reads<NotchRow> = {
  keyOf: (r) => r.key,
  sinceOf: (r) => (r.kind === 'session' ? r.item.since : r.kind === 'review' ? r.review.since : (r.todo.since ?? 0)),
  waitsOf: (r) => (r.kind === 'session' ? statusWaits(r.item.status) : r.kind === 'review' ? (r.review.merge ? 'merges' : 'reviews') : r.waits),
  raisedOf: (r) => (r.kind === 'session' ? r.item.raised : undefined),
  // While it's still Your turn: a pill a window set working ahead of the engine keeps the rest.
  finishedOf: (r) => r.kind === 'session' && r.item.status === 'unread' && !!r.item.finished,
  firstOf: (r) => r.kind === 'review' && !r.review.merge && !!r.review.first,
  // Ready todos go in your order; due ones by their day, the most overdue first, then in yours.
  orderOf: (r) => (r.kind === 'todo' ? r.todo.order : undefined),
  titleOf: (r) => (r.kind === 'session' ? r.item.title : r.kind === 'review' ? reviewTitle(r.review) : r.todo.title || 'Untitled todo'),
};

/** A review as the strip and the queue name it. */
export function reviewTitle(r: Pick<BarReview, 'number' | 'title'>): string {
  return `#${r.number} ${r.title}`;
}

/** Everything the notch can show: what waits on you in the order the queue goes to it (queue.ts
 *  `byPlace`: the ones moved to the top, then by kind in your order, `order`: by default the
 *  sessions that need you, the PRs waiting on your review, the PRs ready to merge, the todos due,
 *  the turns that ended, then the todos ready to start, with the reviews of a repo that puts them
 *  first ahead of them all), then the running ones in the bar's order. ⌥↑ ⌥↓ step along all of it,
 *  so they go the way ⌘] does. */
export function notchRows(items: BarItem[], reviews: BarReview[], todos: { due: BarTodo[]; ready: BarTodo[] } = { due: [], ready: [] }, order: readonly Waits[] = WAITS): NotchRow[] {
  const session = (i: BarItem): NotchRow => ({ kind: 'session', key: i.key, item: i });
  const review = (r: BarReview): NotchRow => ({ kind: 'review', key: r.key, review: r });
  const todo = (waits: 'due' | 'ready') => (t: BarTodo): NotchRow => ({ kind: 'todo', key: todoRowKey(t.id), todo: t, waits });
  const waiting = [...items.filter((i) => i.group !== 'working').map(session), ...reviews.map(review), ...todos.due.map(todo('due')), ...todos.ready.map(todo('ready'))];
  return [...inQueueOrder(waiting, inOrder(ROW_READS, order)), ...items.filter((i) => i.group === 'working').map(session)];
}

/** What the notch works through, as a session's panel in Sessions does: the rows that wait on
 *  you, the running sessions left out. queue.ts has how it goes from one to the next. */
export function waitingRows(rows: NotchRow[]): NotchRow[] {
  return rows.filter((r) => r.kind !== 'session' || r.item.group !== 'working');
}

/** The row the shortcut that works through what waits opens the notch on: the one it showed last
 *  while that still waits on you, else the first that waits in the queue's order (`order`; the
 *  one that has waited longest). None with nothing waiting: the end of what waits says so. */
export function waitingRow(rows: NotchRow[], last: string | null, order: readonly Waits[] = WAITS): NotchRow | null {
  const waiting = inQueueOrder(waitingRows(rows), inOrder(ROW_READS, order));
  return waiting.find((r) => r.key === last) ?? waiting[0] ?? null;
}

/** The row a click on the notch itself opens it on: the shortcut's (`waitingRow`), else the one
 *  it showed last, else the first row. None with nothing on the bar. */
export function currentRow(rows: NotchRow[], last: string | null, order: readonly Waits[] = WAITS): NotchRow | null {
  return waitingRow(rows, last, order) ?? rows.find((r) => r.key === last) ?? rows[0] ?? null;
}
