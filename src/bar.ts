// What the floating bar shows: every session that needs you, whose turn just ended, or is
// running, across all projects, and the PRs waiting for your review. When they don't all fit,
// the least urgent group folds its last pills into one that lists them: Working first, then
// Your turn, then Needs you.

import { withTimes } from './format';
import type { Board, Lane, PrItem, Session, Status } from './types';

export type Group = 'needs' | 'unread' | 'working';
/** Bar order, most urgent first. */
export const GROUPS: Group[] = ['needs', 'unread', 'working'];
/** The order groups fold in when the bar runs out of room. */
const FOLD_ORDER: Group[] = ['working', 'unread', 'needs'];

export interface BarItem {
  key: string;
  group: Group;
  lane: Lane;
  session: Session;
}

export function groupOf(status: Status): Group | null {
  if (status === 'needs' || status === 'error') return 'needs';
  if (status === 'unread' || status === 'working') return status;
  return null;
}

/**
 * The sessions the bar shows, most urgent group first and, within a group, the newest status
 * first. Status times don't move while a status lasts, so pills stay put while agents work.
 * Lanes the board keeps out of sight (Elsewhere) stay out of the bar too.
 */
export function barItems(board: Board): BarItem[] {
  const items: BarItem[] = [];
  for (const lane of board.lanes) {
    if (lane.hidden) continue;
    for (const session of lane.sessions) {
      if (session.visitor || session.archived) continue;
      const group = groupOf(session.status);
      if (group) items.push({ key: session.key, group, lane, session });
    }
  }
  const since = (i: BarItem) => i.session.statusSince ?? i.session.lastActivity;
  return items.sort((a, b) => GROUPS.indexOf(a.group) - GROUPS.indexOf(b.group) || since(b) - since(a) || a.key.localeCompare(b.key));
}

/** Open PRs that ask for your review, newest first. */
export function reviewRequests(board: Board): PrItem[] {
  return board.prs.filter((p) => p.reviewRequested && p.state !== 'merged' && p.state !== 'closed').sort((a, b) => b.updatedAt - a.updatedAt);
}

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
