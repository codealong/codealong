// How the floating bar lays out what it shows (the engine picks and orders it: barfeed.rs):
// when the pills don't all fit, the least urgent group folds its last pills into one that lists
// them: Working first, then Your turn, then Needs you. Also what its project menu counts.

import { withTimes } from './format';
import { ALL_PROJECTS, inProject } from './quick';
import { inQueueOrder, statusWaits, WAITS, type Reads } from './queue';
import type { BarGroup, BarItem, BarReview } from './types';

export type Group = BarGroup;
/** Bar order, most urgent first. */
export const GROUPS: Group[] = ['needs', 'unread', 'working'];
/** How long a click on the bar or a desktop pin waits for a second one: one click stays in
 *  Codealong, two open the session where it lives, without the first click's doing showing first. */
export const CLICK_WAIT_MS = 250;
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
  /** The bar's mark, and the project menu's button beside it (0 when there is none). */
  mark: number;
  project: number;
  /** The snoozed, recent, reviews and todos pills at the end, side by side (0 for one that isn't
   *  there). */
  snoozed: number;
  recent: number;
  reviews: number;
  todos: number;
  /** Space between neighbors, a divider's own width, and the bar's padding on each side. */
  gap: number;
  divider: number;
  padding: number;
}

/** The bar's width with the groups laid out as `fits` say. Sections sit apart by dividers. */
export function barWidth(fits: GroupFit[], m: Metrics): number {
  const sections: number[][] = [[m.mark, m.project].filter((w) => w > 0)];
  for (const f of fits) {
    const widths = [...m.pills[f.group].slice(0, f.shown), ...(f.folded > 0 ? [m.fold[f.group]] : [])];
    if (widths.length) sections.push(widths);
  }
  const last = [m.snoozed, m.recent, m.reviews, m.todos].filter((w) => w > 0);
  if (last.length) sections.push(last);
  const pieces = sections.flat();
  const dividers = sections.length - 1;
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
 *  pills, to read and answer. */
export type NotchState = 'closed' | 'hover' | 'pills' | 'session';

export interface NotchSizes {
  closed: Size;
  pills: Size;
  session: Size;
  /** The biggest the shape can get, a session dragged as far as it goes: what everything is laid
   *  out in. */
  full: Size;
  window: Size;
}

/** The docked bar's shape in each state, and its window. With the pills it's as wide as they or
 *  the sides beside the camera need, never narrower than closed; with a session it's the
 *  session's width and as much taller, at the size you dragged it to (`want`: its width, and its
 *  height under the pills) or its usual one, within what the screen has room for. Everything is
 *  laid out once in the biggest it can get (`full`) and shown through a clip (`notchClip`). The
 *  window is that big too, with room for the corners and the shadow, so it keeps its size
 *  whatever shows, a session being dragged bigger included, and never cuts the shadow off.
 *  `room` is the widest the pills can get. At a `zoom` (a factor) the session's card draws bigger
 *  (zoom.ts), so its usual size is as much bigger, within the screen; a size you dragged it to
 *  stays as you left it. The most it can get grows only for a usual size past it, so most zooms
 *  leave the window as it was: a window resized by the engine lands a moment after the page has
 *  laid out for it, and the shape jumped across in between. */
export function notchSizes(notch: { width: number; height: number }, body: Size, side: number, room: number, screen: Size, want?: Size | null, zoom = 1): NotchSizes {
  const closed = { w: notch.width + 2 * NOTCH.wing, h: notch.height };
  // As much wider than closed on each side, in whole points: the clip's edges then fall on whole
  // points, where they're sharp.
  const widened = (w: number) => closed.w + 2 * Math.ceil(Math.max(0, w - closed.w) / 2);
  const pills = { w: widened(Math.max(body.w + 2 * NOTCH.pad, notch.width + 2 * (side + NOTCH.pad))), h: Math.ceil(notch.height + body.h + NOTCH.pad) };
  const across = screen.w - NOTCH.session.clear;
  const roomDown = Math.floor(screen.h - pills.h - NOTCH.session.clear);
  const usual = { w: Math.min(Math.round(NOTCH.session.width * zoom), across), h: Math.min(Math.round(NOTCH.session.height * zoom), roomDown) };
  const narrowest = Math.max(pills.w, widened(usual.w));
  const widest = Math.max(narrowest, widened(Math.min(NOTCH.session.widest, across)));
  const tallest = Math.max(NOTCH.session.least, usual.h, Math.min(NOTCH.session.tallest, roomDown));
  const tall = Math.min(tallest, Math.max(NOTCH.session.least, Math.round(want?.h ?? usual.h)));
  const session = { w: Math.min(widest, Math.max(narrowest, widened(want?.w ?? usual.w))), h: pills.h + tall };
  const full = { w: Math.max(widest, widened(room + 2 * NOTCH.pad)), h: pills.h + tallest };
  const margin = Math.max(NOTCH.ear.open, NOTCH.shadow.side);
  return { closed, pills, session, full, window: { w: full.w + 2 * margin, h: full.h + NOTCH.shadow.bottom } };
}

/** How much of the layout shows: its insets from the sides and the bottom, and the radius of its
 *  bottom corners. Closed it's the notch and its sides, a little wider under the pointer. */
export function notchClip(s: Pick<NotchSizes, 'closed' | 'pills' | 'session' | 'full'>, state: NotchState): { side: number; bottom: number; radius: number } {
  if (state === 'session') return { side: (s.full.w - s.session.w) / 2, bottom: s.full.h - s.session.h, radius: NOTCH.radius.session };
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

/** What the notch counts beside the camera: the bar's groups, and the PRs that wait on your
 *  review. */
export type NotchKind = Group | 'reviews';
/** Most urgent first, as the queue works through what waits (queue.ts `WAITS`), then what runs.
 *  A review asked of you ranks as the mark's colors rank it (board.rs `logo_status`): with what
 *  needs you, after the sessions that do. */
export const NOTCH_ORDER: NotchKind[] = [...WAITS, 'working'];

/** What the closed notch shows beside the camera: the most urgent kind with anything in it and
 *  how many; none when nothing needs you, waits for you or runs. */
export function notchCount(counts: Record<NotchKind, number>): { kind: NotchKind; n: number } | null {
  const kind = NOTCH_ORDER.find((k) => counts[k] > 0);
  return kind ? { kind, n: counts[kind] } : null;
}

// ---- What the notch works through (NotchSession.tsx, NotchReview.tsx) ----

/** A row of what the notch shows under its pills: a session on the bar, or a PR that waits on
 *  your review. Keyed by the session's key or the PR's (`owner/name#12`), which never collide. */
export type NotchRow = { kind: 'session'; key: string; item: BarItem } | { kind: 'review'; key: string; review: BarReview };

/** How the queue (queue.ts) reads the notch's rows: a pill carries its session's key and times
 *  without the session, a review the PR's. */
export const ROW_READS: Reads<NotchRow> = {
  keyOf: (r) => r.key,
  sinceOf: (r) => (r.kind === 'session' ? r.item.since : r.review.since),
  waitsOf: (r) => (r.kind === 'session' ? statusWaits(r.item.status) : 'reviews'),
  titleOf: (r) => (r.kind === 'session' ? r.item.title : reviewTitle(r.review)),
};

/** A review as the strip and the queue name it. */
export function reviewTitle(r: Pick<BarReview, 'number' | 'title'>): string {
  return `#${r.number} ${r.title}`;
}

/** Everything the notch can show, in the order the closed notch counts it (`NOTCH_ORDER`): the
 *  sessions that need you, the PRs waiting on your review, the turns that ended, then the
 *  running ones, each group in the bar's own order. ⌥↑ ⌥↓ step along all of it. */
export function notchRows(items: BarItem[], reviews: BarReview[]): NotchRow[] {
  const session = (i: BarItem): NotchRow => ({ kind: 'session', key: i.key, item: i });
  const review = (r: BarReview): NotchRow => ({ kind: 'review', key: r.key, review: r });
  return NOTCH_ORDER.flatMap((k) => (k === 'reviews' ? reviews.map(review) : items.filter((i) => i.group === k).map(session)));
}

/** What the notch works through, as a session's panel in Sessions does: the rows that wait on
 *  you, the running sessions left out. queue.ts has how it goes from one to the next. */
export function waitingRows(rows: NotchRow[]): NotchRow[] {
  return rows.filter((r) => r.kind !== 'session' || r.item.group !== 'working');
}

/** The row a click on the notch itself opens it on: the one it showed last while that still
 *  waits on you, else the first that waits in the queue's order (the one that has waited
 *  longest), else the one it showed last, else the first row. None with nothing on the bar. */
export function currentRow(rows: NotchRow[], last: string | null): NotchRow | null {
  const waiting = inQueueOrder(waitingRows(rows), ROW_READS);
  return waiting.find((r) => r.key === last) ?? waiting[0] ?? rows.find((r) => r.key === last) ?? rows[0] ?? null;
}
