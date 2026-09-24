// Lanes on the board: grouped into bays, seen through the age filter, and matched against the
// Inbox filters. The engine sets a lane's bay and headline from all of its sessions; when the
// age filter hides some of them, the lane's status and sentence are worked out again from
// what's left, with the same rules as the engine (board.rs, `bay_of` and `headline`).

import type { Bay, Lane, Pr, Session, Status } from './types';

export const BAYS: Bay[] = ['needs', 'working', 'unread', 'review', 'idle', 'merged'];
/** The bay an agent's own row goes in, for the statuses that get one. */
export const AGENT_BAY: Partial<Record<Status, Bay>> = { needs: 'needs', error: 'needs', working: 'working', unread: 'unread' };

/** A row on the Worktrees board or in Sessions: an agent in its worktree, or a worktree. */
export type BoardRow = { kind: 'agent'; key: string; lane: Lane; session: Session } | { kind: 'lane'; key: string; lane: Lane };
/** What the Inbox narrows the board to: one bay, or worktrees whose PR checks are failing. */
export type Filter = Bay | 'failing';

function prOpen(pr: Pr): boolean {
  return pr.state !== 'merged' && pr.state !== 'closed';
}

/** The lane's open PR when its checks are failing. */
export function failingPr(lane: Lane): Pr | null {
  const pr = lane.pr;
  return pr && prOpen(pr) && pr.checks.failed > 0 ? pr : null;
}

/** The first rule that matches, as in the engine: agents that need you, then the PR, then work. */
export function bayOf(sessions: Session[], pr: Pr | null): [Bay, Status] {
  const has = (st: Status) => sessions.some((s) => s.status === st);
  const working = has('working');
  if (has('needs')) return ['needs', 'needs'];
  if (has('error')) return ['needs', 'error'];
  if (pr && prOpen(pr) && !working) {
    if (pr.checks.failed > 0) return ['needs', 'error'];
    if (pr.state === 'changes') return ['needs', 'needs'];
  }
  if (working) return ['working', 'working'];
  if (has('unread')) return ['unread', 'unread'];
  if (pr) return prOpen(pr) ? ['review', 'idle'] : ['merged', 'idle'];
  return ['idle', 'idle'];
}

/**
 * The sentence for a lane whose status changed under the filter. As in the engine (board.rs,
 * `headline`), an agent bay takes its sentence from the newest agent with that status; the
 * rest get a plain one.
 */
function headlineFor(bay: Bay, status: Status, own: Session[], pr: Pr | null, since: string): [string, string | null] {
  const first = (...statuses: Status[]) => statuses.map((st) => own.find((s) => s.status === st)).find((s) => s != null);
  const agent = bay === 'needs' ? first('needs', 'error') : bay === 'working' || bay === 'unread' ? first(bay) : undefined;
  if (agent) return [agent.headline, agent.detail];
  if (pr && bay === 'needs') return [status === 'error' ? `Checks failing on #${pr.number}` : `Changes requested on #${pr.number}`, null];
  if (pr && bay === 'merged') return [`#${pr.number} ${pr.state === 'closed' ? 'closed' : 'merged'}`, null];
  if (pr && bay === 'review') return [`#${pr.number} ${pr.title}`, null];
  return [`No agents active ${since}`, null];
}

/**
 * The lane with only the sessions active since `cutoff`. When that changes its status, the bay,
 * glyph and headline are worked out again.
 */
export function ageFiltered(lane: Lane, cutoff: number, since: string): Lane {
  if (cutoff <= 0) return lane;
  const sessions = lane.sessions.filter((s) => s.lastActivity >= cutoff);
  if (sessions.length === lane.sessions.length) return lane;
  const own = sessions.filter((s) => !s.visitor);
  const [bay, glyph] = bayOf(own, lane.pr);
  if (bay === lane.bay && glyph === lane.glyph) return { ...lane, sessions };
  const [headline, detail] = headlineFor(bay, glyph, own, lane.pr, since);
  return { ...lane, sessions, bay, glyph, headline, detail };
}

/**
 * Worktrees active since `cutoff`, and pinned ones however long they've been quiet, each
 * showing only its sessions active since then. Pinned sessions don't count here: they belong
 * to the Sessions view. A cutoff of 0 filters nothing.
 */
export function filterByAge(lanes: Lane[], cutoff: number, pinnedLanes: Set<string>, since: string): Lane[] {
  if (cutoff <= 0) return lanes;
  return lanes.filter((l) => l.lastActivity >= cutoff || pinnedLanes.has(l.id)).map((l) => ageFiltered(l, cutoff, since));
}

/** Agents go in the agent bays, one row each; worktrees go in the rest, one row each. */
export function buildBays(lanes: Lane[]): { bay: Bay; rows: BoardRow[] }[] {
  const groups: Record<Bay, BoardRow[]> = { needs: [], working: [], unread: [], review: [], idle: [], merged: [] };
  for (const lane of lanes) {
    const own = lane.sessions.filter((s) => !s.visitor);
    for (const s of own) {
      const bay = AGENT_BAY[s.status];
      if (bay) groups[bay].push({ kind: 'agent', key: `${lane.id}::${s.key}`, lane, session: s });
    }
    const needsFromAgents = own.some((s) => s.status === 'needs' || s.status === 'error');
    const agentBay = lane.bay === 'working' || lane.bay === 'unread' || (lane.bay === 'needs' && needsFromAgents);
    if (!agentBay) groups[lane.bay].push({ kind: 'lane', key: lane.id, lane });
  }
  const at = (r: BoardRow) => (r.kind === 'agent' ? r.session.lastActivity : r.lane.lastActivity);
  return BAYS.map((bay) => ({ bay, rows: groups[bay].sort((a, b) => at(b) - at(a)) })).filter((g) => g.rows.length > 0);
}

/**
 * The Inbox filters, applied to a whole worktree: it matches when any of its rows on the board
 * would. A pinned worktree shows as a single row, so this decides whether that row shows.
 */
export function laneInFilter(lane: Lane, f: Filter | null): boolean {
  if (!f) return true;
  if (f === 'failing') return failingPr(lane) != null;
  return buildBays([lane]).some((g) => g.bay === f);
}
