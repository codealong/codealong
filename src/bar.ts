// How the floating bar lays out what it shows (the engine picks and orders it: barfeed.rs):
// when the pills don't all fit, the least urgent group folds its last pills into one that lists
// them: Working first, then Your turn, then Needs you. Also what its project menu counts.

import { withTimes } from './format';
import { ALL_PROJECTS, inProject, leftAfter, type Handled } from './quick';
import type { BarGroup, BarItem } from './types';

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
 *  padding around the pills; the session under them, no narrower than `width`, `height` tall
 *  where the screen has the room and never less than `least`, leaving `clear` of the screen under
 *  it; and the room the window keeps beside and under the shape for its shadow (app.css
 *  `--shadow-notch` reaches 16 beside it and 30 under it). */
export const NOTCH = {
  wing: 44,
  hover: 4,
  ear: { closed: 6, open: 14 },
  radius: { closed: 12, open: 22, session: 26 },
  pad: 10,
  session: { width: 560, height: 520, least: 240, clear: 96 },
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
  window: Size;
}

/** The docked bar's shape in each state, and its window. With the pills it's as wide as they or
 *  the sides beside the camera need, never narrower than closed; with a session it's at least
 *  the session's width, and as much taller. Everything is laid out once in the biggest of them,
 *  the session's, and shown through a clip (`notchClip`). The window is as wide as the pills can
 *  get (`room`) and as tall as the session opens on a screen `screenH` high, with room for the
 *  corners and the shadow, so it keeps its size whatever shows and never cuts the shadow off. */
export function notchSizes(notch: { width: number; height: number }, body: Size, side: number, room: number, screenH: number): NotchSizes {
  const closed = { w: notch.width + 2 * NOTCH.wing, h: notch.height };
  // As much wider than closed on each side, in whole points: the clip's edges then fall on whole
  // points, where they're sharp.
  const widened = (w: number) => closed.w + 2 * Math.ceil(Math.max(0, w - closed.w) / 2);
  const pills = { w: widened(Math.max(body.w + 2 * NOTCH.pad, notch.width + 2 * (side + NOTCH.pad))), h: Math.ceil(notch.height + body.h + NOTCH.pad) };
  const tall = Math.max(NOTCH.session.least, Math.min(NOTCH.session.height, Math.floor(screenH - pills.h - NOTCH.session.clear)));
  const session = { w: widened(Math.max(pills.w, NOTCH.session.width)), h: pills.h + tall };
  const margin = Math.max(NOTCH.ear.open, NOTCH.shadow.side);
  return { closed, pills, session, window: { w: Math.ceil(Math.max(session.w, room + 2 * NOTCH.pad)) + 2 * margin, h: session.h + NOTCH.shadow.bottom } };
}

/** How much of the layout shows: its insets from the sides and the bottom, and the radius of its
 *  bottom corners. Closed it's the notch and its sides, a little wider under the pointer. */
export function notchClip(s: Pick<NotchSizes, 'closed' | 'pills' | 'session'>, state: NotchState): { side: number; bottom: number; radius: number } {
  if (state === 'session') return { side: 0, bottom: 0, radius: NOTCH.radius.session };
  if (state === 'pills') return { side: (s.session.w - s.pills.w) / 2, bottom: s.session.h - s.pills.h, radius: NOTCH.radius.open };
  const side = Math.max(0, (s.session.w - s.closed.w) / 2 - (state === 'hover' ? NOTCH.hover : 0));
  return { side, bottom: s.session.h - s.closed.h, radius: NOTCH.radius.closed };
}

/** What the notch counts beside the camera: the bar's groups, and the PRs that wait on your
 *  review. */
export type NotchKind = Group | 'reviews';
/** Most urgent first. A review asked of you ranks as the mark's colors rank it
 *  (board.rs `logo_status`): with what needs you, after the sessions that do. */
export const NOTCH_ORDER: NotchKind[] = ['needs', 'reviews', 'unread', 'working'];

/** What the closed notch shows beside the camera: the most urgent kind with anything in it and
 *  how many; none when nothing needs you, waits for you or runs. */
export function notchCount(counts: Record<NotchKind, number>): { kind: NotchKind; n: number } | null {
  const kind = NOTCH_ORDER.find((k) => counts[k] > 0);
  return kind ? { kind, n: counts[kind] } : null;
}

// ---- The session in the notch (NotchSession.tsx) ----

/** What the session in the notch works through, as a session's panel in Sessions does: the bar's
 *  sessions that wait on you, in the bar's order (Needs you the longest wait first, then Your
 *  turn the newest first). */
export function waitingItems(items: BarItem[]): BarItem[] {
  return items.filter((i) => i.group !== 'working');
}

/** What's left of them after the session `key`, the next first (quick.ts `queueAfter`). */
export function waitingAfter(items: BarItem[], key: string, handled: Handled): BarItem[] {
  return leftAfter(waitingItems(items), key, handled, (i) => i.key, (i) => i.since);
}

/** The session a click on the notch itself opens it on: the one it showed last while that still
 *  waits on you, else the first that waits, else the one it showed last, else the bar's first.
 *  None with no sessions on the bar. */
export function currentItem(items: BarItem[], last: string | null): BarItem | null {
  const waiting = waitingItems(items);
  return waiting.find((i) => i.key === last) ?? waiting[0] ?? items.find((i) => i.key === last) ?? items[0] ?? null;
}

/** The session `delta` along the bar from the session `key`, stopping at its ends; the first, or
 *  the last going back, from one the bar doesn't list. */
export function stepItem(items: BarItem[], key: string, delta: 1 | -1): BarItem | null {
  if (items.length === 0) return null;
  const i = items.findIndex((x) => x.key === key);
  if (i < 0) return delta > 0 ? items[0] : items[items.length - 1];
  return items[Math.min(items.length - 1, Math.max(0, i + delta))];
}
