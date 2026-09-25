// How the floating bar lays out what it shows (the engine picks and orders it: barfeed.rs):
// when the pills don't all fit, the least urgent group folds its last pills into one that lists
// them: Working first, then Your turn, then Needs you.

import { withTimes } from './format';
import type { BarGroup } from './types';

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
  /** The bar's mark, and the reviews pill (0 when there is none). */
  mark: number;
  reviews: number;
  /** Space between neighbors, a divider's own width, and the bar's padding on each side. */
  gap: number;
  divider: number;
  padding: number;
}

/** The bar's width with the groups laid out as `fits` say. Sections sit apart by dividers. */
export function barWidth(fits: GroupFit[], m: Metrics): number {
  const sections: number[][] = [[m.mark]];
  for (const f of fits) {
    const widths = [...m.pills[f.group].slice(0, f.shown), ...(f.folded > 0 ? [m.fold[f.group]] : [])];
    if (widths.length) sections.push(widths);
  }
  if (m.reviews > 0) sections.push([m.reviews]);
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
