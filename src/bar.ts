// How the floating bar lays out what it shows (the engine picks and orders it: barfeed.rs):
// when the pills don't all fit, the least urgent group folds its last pills into one that lists
// them: Working first, then Your turn, then Needs you. Also what its project menu counts.

import { withTimes } from './format';
import { ALL_PROJECTS, inProject } from './quick';
import type { BarGroup, BarItem } from './types';

export type Group = BarGroup;
/** Bar order, most urgent first. */
export const GROUPS: Group[] = ['needs', 'unread', 'working'];
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

/** The docked bar's measures, in points: each side beside the camera while it's closed; the
 *  concave corners where it meets the menu bar and its bottom corners, closed and open; the
 *  padding around what opens below; and the room under the open shape for its shadow, which the
 *  peek counts on to open under it (peek.rs `BAR_MARGIN`). */
export const NOTCH = {
  wing: 44,
  ear: { closed: 6, open: 14 },
  radius: { closed: 12, open: 22 },
  pad: 10,
  bottom: 12,
};

export interface Size {
  w: number;
  h: number;
}

/** The docked bar's shape, closed and open, and its window for each: the shape with room for
 *  the corners at its top, and open, for the shadow under it. Open, it's as wide as the pills
 *  below it or the sides beside the camera need, and never narrower than closed. */
export function notchSizes(notch: { width: number; height: number }, body: Size, side: number): { closed: Size; open: Size; closedWindow: Size; openWindow: Size } {
  const closed = { w: notch.width + 2 * NOTCH.wing, h: notch.height };
  const open = {
    w: Math.ceil(Math.max(closed.w, body.w + 2 * NOTCH.pad, notch.width + 2 * (side + NOTCH.pad))),
    h: Math.ceil(notch.height + body.h + NOTCH.pad),
  };
  return {
    closed,
    open,
    closedWindow: { w: closed.w + 2 * NOTCH.ear.closed, h: closed.h },
    openWindow: { w: open.w + 2 * NOTCH.ear.open, h: open.h + NOTCH.bottom },
  };
}

/** What the closed notch shows beside the camera: the most urgent group with anything in it, in
 *  the bar's order, and how many; none when nothing needs you, waits for you or runs. */
export function notchCount(counts: Record<Group, number>): { group: Group; n: number } | null {
  const group = GROUPS.find((g) => counts[g] > 0);
  return group ? { group, n: counts[group] } : null;
}
