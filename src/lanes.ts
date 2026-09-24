// A lane seen through the age filter. The engine sets a lane's bay and headline from all of
// its sessions; when the filter hides some of them, the lane's status is worked out again from
// what's left, with the same rules as the engine (board.rs, `bay_of`).

import type { Bay, Lane, Pr, Session, Status } from './types';

function prOpen(pr: Pr): boolean {
  return pr.state !== 'merged' && pr.state !== 'closed';
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

/** A plain headline for a lane whose status changed under the filter. */
function headlineFor(bay: Bay, status: Status, pr: Pr | null, since: string): string {
  if (pr && bay === 'needs') return status === 'error' ? `Checks failing on #${pr.number}` : `Changes requested on #${pr.number}`;
  if (pr && bay === 'merged') return `#${pr.number} ${pr.state === 'closed' ? 'closed' : 'merged'}`;
  if (pr && bay === 'review') return `#${pr.number} ${pr.title}`;
  return `No agents active ${since}`;
}

/**
 * The lane with only the sessions active since `cutoff` (and pinned ones). When that changes
 * its status, the bay, glyph and headline are worked out again.
 */
export function ageFiltered(lane: Lane, cutoff: number, pinned: Set<string>, since: string): Lane {
  if (cutoff <= 0) return lane;
  const sessions = lane.sessions.filter((s) => s.lastActivity >= cutoff || pinned.has(s.key));
  if (sessions.length === lane.sessions.length) return lane;
  const [bay, glyph] = bayOf(
    sessions.filter((s) => !s.visitor),
    lane.pr,
  );
  if (bay === lane.bay && glyph === lane.glyph) return { ...lane, sessions };
  return { ...lane, sessions, bay, glyph, headline: headlineFor(bay, glyph, lane.pr, since), detail: null };
}
