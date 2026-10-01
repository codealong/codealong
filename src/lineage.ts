// Where sessions came from (`Session.origin`): a fork of another session, or started by another
// session's agent. The Canvas view and the cards pinned to the desktop draw a line from the
// session it came from to the one that came of it while either is hovered or selected; this is
// the line's shape, which both draw the same way.
//
// Cards often sit in a column, one under the other, where a straight line between their middles
// would run under the cards and show only in the gap between them. So the line leaves and meets
// the cards at their sides: the sides that face each other when there's room between them, else
// both on one side, bowing out past the cards.

import type { Placed } from './lanes';
import type { Session, SessionOrigin } from './types';

export interface Pt {
  x: number;
  y: number;
}

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** A cubic Bézier: from `a` to `b`, pulled toward `c1` and `c2`. */
export interface Curve {
  a: Pt;
  c1: Pt;
  c2: Pt;
  b: Pt;
}

/** How far apart two cards side by side must be for the line to go straight across. */
const ACROSS_MIN = 16;

/** The line from the card `from` to the card `to`, in the same coordinates as the boxes (y down).
 *  `side` is where it bows out when they're one over the other: left unless told there's more room
 *  on the right. */
export function connector(from: Box, to: Box, side: 'left' | 'right' = 'left'): Curve {
  const midY = (r: Box) => r.y + r.h / 2;
  if (to.x - (from.x + from.w) >= ACROSS_MIN || from.x - (to.x + to.w) >= ACROSS_MIN) {
    const rightward = to.x > from.x;
    const a = { x: rightward ? from.x + from.w : from.x, y: midY(from) };
    const b = { x: rightward ? to.x : to.x + to.w, y: midY(to) };
    const pull = Math.max(24, Math.abs(b.x - a.x) / 2) * (rightward ? 1 : -1);
    return { a, c1: { x: a.x + pull, y: a.y }, c2: { x: b.x - pull, y: b.y }, b };
  }
  const bow = Math.min(90, 28 + Math.abs(midY(to) - midY(from)) * 0.15);
  if (side === 'right') {
    const a = { x: from.x + from.w, y: midY(from) };
    const b = { x: to.x + to.w, y: midY(to) };
    const out = Math.max(a.x, b.x) + bow;
    return { a, c1: { x: out, y: a.y }, c2: { x: out, y: b.y }, b };
  }
  const a = { x: from.x, y: midY(from) };
  const b = { x: to.x, y: midY(to) };
  const out = Math.min(a.x, b.x) - bow;
  return { a, c1: { x: out, y: a.y }, c2: { x: out, y: b.y }, b };
}

/** The point `t` of the way along a curve. */
export function pointAt(c: Curve, t: number): Pt {
  const u = 1 - t;
  const [k0, k1, k2, k3] = [u * u * u, 3 * u * u * t, 3 * u * t * t, t * t * t];
  return { x: k0 * c.a.x + k1 * c.c1.x + k2 * c.c2.x + k3 * c.b.x, y: k0 * c.a.y + k1 * c.c1.y + k2 * c.c2.y + k3 * c.b.y };
}

/** The curve as `n` straight pieces, for drawing with lines. */
export function polyline(c: Curve, n = 20): Pt[] {
  return Array.from({ length: n + 1 }, (_, i) => pointAt(c, i / n));
}

/** The arrowhead at the curve's end: its two barbs, from the tip back along the way it came. */
export function arrowhead(c: Curve, length = 8, spread = 0.5): [Pt, Pt] {
  // The direction it arrives from: toward the last control point, or the one before when they meet.
  const from = c.c2.x !== c.b.x || c.c2.y !== c.b.y ? c.c2 : c.c1;
  const angle = Math.atan2(from.y - c.b.y, from.x - c.b.x);
  const barb = (d: number) => ({ x: c.b.x + length * Math.cos(angle + d), y: c.b.y + length * Math.sin(angle + d) });
  return [barb(spread), barb(-spread)];
}

/** The curve as an SVG path. */
export function pathOf(c: Curve): string {
  const p = (q: Pt) => `${q.x.toFixed(1)} ${q.y.toFixed(1)}`;
  return `M ${p(c.a)} C ${p(c.c1)}, ${p(c.c2)}, ${p(c.b)}`;
}

/** How a session came of another, as a phrase before that one's title. */
export function originWord(how: SessionOrigin['how']): string {
  return how === 'fork' ? 'Forked from' : 'Started by';
}

/** A session on the board and how it came of another, or that one of it. */
export type Kin = Placed & { how: SessionOrigin['how'] };

/** Where a session came from and what came of it, as far as the board has them. */
export interface Lineage {
  from: Kin | null;
  to: Kin[];
}

/** A session's lineage, looked up among the board's sessions where they live (`sessionHomes`). */
export function lineageOf(session: Session, homes: Map<string, Placed>): Lineage {
  const from = session.origin ? homes.get(session.origin.key) : undefined;
  return {
    from: from && session.origin ? { ...from, how: session.origin.how } : null,
    to: cameOf(session.key, homes.values()).map((p) => ({ ...p, how: p.session.origin?.how ?? 'started' })),
  };
}

/** The sessions that came of `key`, among `sessions`: forks of it and ones its agent started. */
export function cameOf<T extends { session: Session }>(key: string, sessions: Iterable<T>): T[] {
  const out: T[] = [];
  for (const x of sessions) if (x.session.origin?.key === key) out.push(x);
  return out;
}
