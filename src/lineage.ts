// Where sessions came from (`Session.origin`): a fork of another session (or a move, from
// another account), or started by another session's agent. The Canvas view and the cards pinned to the desktop draw a line from the
// session it came from to the one that came of it while either is hovered or selected; this is
// the line's shape, which both draw the same way.
//
// Cards often sit in a column, one under the other, where a straight line between their middles
// would run under the cards and show only in the gap between them. So the line leaves and meets
// the cards at their sides: the sides that face each other when there's room between them, else
// both on one side, bowing out past the cards.

import { sideTitle, type Placed } from './lanes';
import type { Board, Session, SessionOrigin, SideGlance, Starting } from './types';

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

/** How a session came of another, as the windows tell it. A fork into another account of its
 *  harness is a move (start.rs `move_to`), told by the accounts alone: the shared model has no
 *  word for it, so devices on older builds still read the origin. A session another's agent
 *  started as its helper (`Session.helper`) is that one's helper. */
export type KinHow = SessionOrigin['how'] | 'moved' | 'helper';

/** How `to` came of `from`, its origin saying `how`. */
export function kinHow(how: SessionOrigin['how'], from: Session, to: Session): KinHow {
  if (to.helper?.parent === from.key) return 'helper';
  return how === 'fork' && from.account !== to.account ? 'moved' : how;
}

/** How a session came of another, as a phrase before that one's title. */
export function originWord(how: KinHow): string {
  return how === 'fork' ? 'Forked from' : how === 'moved' ? 'Moved from' : how === 'helper' ? 'Helper of' : 'Started by';
}

/** What came of a session, as a phrase before that one's title. */
export function cameWord(how: KinHow): string {
  return how === 'fork' ? 'Forked into' : how === 'moved' ? 'Moved to' : how === 'helper' ? 'Its helper' : 'Started';
}

/** A session on the board and how it came of another, or that one of it. */
export type Kin = Placed & { how: KinHow };

/** Where a session came from and what came of it, as far as the board has them: the sessions
 *  (`to`), the headless runs its commands started apart from them (`runs`), since they come by the
 *  hundred, the side conversations asked about it (`sides`, `Board.sides`), which aren't
 *  sessions, and the starts its agent asked for whose sessions aren't here yet (`starting`,
 *  `Board.starting`). */
export interface Lineage {
  from: Kin | null;
  to: Kin[];
  runs: Kin[];
  sides: SideGlance[];
  starting: Starting[];
}

/** A session's lineage, looked up among the board's sessions where they live (`sessionHomes`),
 *  and its side conversations and the starts still on their way among the board's. */
export function lineageOf(session: Session, homes: Map<string, Placed>, board?: Pick<Board, 'sides' | 'starting'> | null): Lineage {
  const helperParent = session.helper?.parent && !session.helper.pr ? homes.get(session.helper.parent) : undefined;
  const originFrom = session.origin ? homes.get(session.origin.key) : undefined;
  const from = helperParent
    ? { ...helperParent, how: 'helper' as const }
    : originFrom && session.origin
      ? { ...originFrom, how: kinHow(session.origin.how, originFrom.session, session) }
      : null;
  return {
    from,
    to: cameOf(session.key, homes.values()).map((p) => ({ ...p, how: kinHow(p.session.origin?.how ?? 'started', session, p.session) })),
    runs: [...homes.values()].filter((p) => p.session.headless && p.session.origin?.key === session.key && p.session.helper?.parent !== session.key).map((p) => ({ ...p, how: 'started' as const })),
    sides: board?.sides?.[session.key] ?? [],
    starting: board?.starting?.[session.key] ?? [],
  };
}

/** A start on its way, by how its prompt opens, which its session will likely be titled by. */
export function startingTitle(s: Starting): string {
  return s.opens ?? `${s.harness} on ${s.account}`;
}

/** The first non-blank line a stuck start's terminal shows, which says what it waits on (Codex's
 *  “Hooks need review”). */
export function screenHeadline(screen: string): string {
  return (screen.split('\n').find((l) => l.trim() !== '') ?? '').trim();
}

/** Where a start whose session isn't here yet stands, on one short phrase: what its terminal waits
 *  on, Waiting in its terminal, or Starting. */
export function startingState(s: Starting): string {
  if (!s.stuck) return 'Starting';
  const headline = s.screen ? screenHeadline(s.screen) : '';
  return headline ? `“${headline}”` : `Waiting in ${s.terminal ?? 'its terminal'}`;
}

/** A session's pending starts, in the order they appear under it: stuck ones first, then newest
 *  first. */
export function sortStarting(starts: readonly Starting[] | undefined): Starting[] {
  if (!starts || starts.length === 0) return [];
  return [...starts].sort((a, b) => Number(b.stuck) - Number(a.stuck) || b.at - a.at);
}

/** One line under a session's title for what came of it (SessionHeader.tsx): a session, its
 *  headless runs together (the latest first), a side conversation, or a start whose session
 *  isn't here yet, with when it started. */
export type CameLine = { kind: 'kin'; kin: Kin; at: number } | { kind: 'runs'; runs: Kin[]; at: number } | { kind: 'side'; side: SideGlance; at: number } | { kind: 'starting'; starting: Starting; at: number };

/** What came of a session, a line each, the latest started first: it's the one the header shows,
 *  and the rest open under it in the order they started, so the list holds still as they work.
 *  A start stuck in its terminal comes before them all: it waits on you, so it isn't folded away. */
export function cameLines(lineage: Pick<Lineage, 'to' | 'runs' | 'sides' | 'starting'> | null | undefined): CameLine[] {
  const started = (s: Session) => s.startedAt ?? s.lastPromptAt;
  const lines: CameLine[] = [
    ...(lineage?.to ?? []).map((kin): CameLine => ({ kind: 'kin', kin, at: started(kin.session) })),
    ...(lineage?.sides ?? []).map((side): CameLine => ({ kind: 'side', side, at: side.startedAt })),
    ...(lineage?.starting ?? []).map((starting): CameLine => ({ kind: 'starting', starting, at: starting.at })),
  ];
  const runs = [...(lineage?.runs ?? [])].sort((a, b) => started(b.session) - started(a.session));
  if (runs.length > 0) lines.push({ kind: 'runs', runs, at: started(runs[0].session) });
  const waits = (l: CameLine) => l.kind === 'starting' && l.starting.stuck;
  return lines.sort((a, b) => Number(waits(b)) - Number(waits(a)) || b.at - a.at);
}

/** A line of what came of a session in words, for the tooltip that lists the ones folded away. */
export function cameLineWords(line: CameLine): string {
  if (line.kind === 'kin') return `${cameWord(line.kin.how)} ${line.kin.session.title}`;
  if (line.kind === 'side') return `On the side: ${sideTitle(line.side)}`;
  if (line.kind === 'starting') return `${line.starting.helper ? 'Its helper' : 'Started'}, starting: ${startingTitle(line.starting)}`;
  return line.runs.length === 1 ? 'A headless run' : `${line.runs.length} headless runs`;
}

/** The sessions that came of `key`, among `sessions`: forks of it, ones its agent started, and its
 *  helpers, which include the automatic reviews of a PR it opened (autoreview.rs `nest_own`). Not
 *  the headless runs its commands started, which come by the hundred: its row counts those
 *  (boardLists.ts `runCounts`). */
export function cameOf<T extends { session: Session }>(key: string, sessions: Iterable<T>): T[] {
  const out: T[] = [];
  for (const x of sessions) if ((x.session.origin?.key === key && !x.session.headless) || x.session.helper?.parent === key) out.push(x);
  return out;
}

/** For every session on the board that moved to another account (a fork onto a different account),
 *  maps its key to the latest destination copy still on the board, following any chain of moves.
 *  Copies of the same source share its first transcript timestamp once closed, so ties break by
 *  unarchived, live, lastActivity, then start time. */
export function movedTargets(homes: Map<string, Placed>): Map<string, Placed> {
  const started = (s: Session) => s.startedAt ?? s.lastPromptAt;
  const better = (a: Session, b: Session): boolean => {
    if (a.archived !== b.archived) return !a.archived;
    if (a.live !== b.live) return a.live;
    if (a.lastActivity !== b.lastActivity) return a.lastActivity > b.lastActivity;
    if (started(a) !== started(b)) return started(a) > started(b);
    return a.key > b.key;
  };
  const direct = new Map<string, Placed>();
  for (const p of homes.values()) {
    const origin = p.session.origin;
    if (!origin || p.session.headless) continue;
    const from = homes.get(origin.key);
    if (!from || kinHow(origin.how, from.session, p.session) !== 'moved') continue;
    const prev = direct.get(origin.key);
    if (!prev || better(p.session, prev.session)) direct.set(origin.key, p);
  }
  const out = new Map<string, Placed>();
  for (const [key, first] of direct) {
    let cur = first;
    const seen = new Set<string>([key, cur.session.key]);
    while (true) {
      const next = direct.get(cur.session.key);
      if (!next || seen.has(next.session.key)) break;
      seen.add(next.session.key);
      cur = next;
    }
    out.set(key, cur);
  }
  return out;
}
