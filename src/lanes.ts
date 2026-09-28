// Lanes on the board: grouped into bays, seen through the age and Inbox filters, and sorted.
// The engine sets a lane's bay and headline from all of its sessions; when a filter hides some
// of them, the lane's status and sentence are worked out again from what's left, with the same
// rules as the engine (board.rs, `bay_of` and `headline`).

import type { Bay, Lane, Pr, Session, Status } from './types';

export const BAYS: Bay[] = ['needs', 'working', 'unread', 'review', 'idle', 'merged'];
/** The Inbox state an agent is in, for the statuses that have one. */
export const AGENT_BAY: Partial<Record<Status, Bay>> = { needs: 'needs', error: 'needs', working: 'working', unread: 'unread' };
/** How urgent a status is, as the engine ranks it. Idle and stopped rank the same: a Codex
 *  thread is never stopped, so telling them apart would only sort the harnesses apart. */
const URGENCY: Record<Status, number> = { needs: 0, error: 1, working: 2, unread: 3, idle: 4, stopped: 4 };

/** An agent in the worktree it runs in: a line in a worktree's row, or a row in Sessions. */
export type AgentItem = { kind: 'agent'; key: string; lane: Lane; session: Session };
/** A worktree with no agent that needs you, is working or just finished. */
export type LaneItem = { kind: 'lane'; key: string; lane: Lane };
/** Something you can select on the Worktrees board or in Sessions. */
export type BoardRow = AgentItem | LaneItem;
/**
 * A worktree's row on the board: a line for each of its agents that needs you, is working or
 * just finished, or, with none, the worktree alone.
 */
export interface WorktreeRow {
  key: string;
  lane: Lane;
  agents: AgentItem[];
}
/** What the Inbox narrows the board to: one bay, or worktrees whose PR checks are failing. */
export type Filter = Bay | 'failing';

/** When a session began waiting on you, for one that needs you, as the engine has it
 *  (`waiting_since`): Needs you is a queue, the longest wait first, so a new one can't keep
 *  pushing an old one down. */
export function waitingSince(s: Session): number | null {
  return s.status === 'needs' || s.status === 'error' ? (s.statusSince ?? s.lastActivity) : null;
}

/** When a session's status began. */
const statusAt = (s: Session) => s.statusSince ?? s.lastActivity;

/**
 * Two sessions of one of the floating bar's groups, in its order (barfeed.rs): Needs you the
 * longest wait first; Your turn and Working, which read as a feed, the newest status first. A
 * status's time doesn't move while it lasts, so rows hold their places.
 */
export function byStatusTime(a: Session, b: Session): number {
  const d = statusAt(a) - statusAt(b);
  return waitingSince(a) !== null ? d : -d;
}

/**
 * Most urgent first, as the engine sorts a lane's sessions; those that need you by how long
 * they've waited, the rest by the one you last sent a prompt. Activity would reorder working
 * agents with every tool call; your prompts only move one when you do something.
 */
export function bySession(a: Session, b: Session): number {
  return URGENCY[a.status] - URGENCY[b.status] || (waitingSince(a) ?? 0) - (waitingSince(b) ?? 0) || b.lastPromptAt - a.lastPromptAt || b.lastActivity - a.lastActivity;
}

/** A finished turn waiting on you to read it: Your turn, or a reply that asks you something.
 *  Marking it seen makes it idle, which takes it off the bar and out of its bay. */
export function seenable(s: Session): boolean {
  return s.status === 'unread' || !!s.asks;
}

/** When you last sent a prompt to one of a worktree's own agents; with none, when anything last happened in it. */
export function lanePromptAt(lane: Lane): number {
  const own = lane.sessions.filter((s) => !s.visitor && !s.archived);
  return own.length > 0 ? Math.max(...own.map((s) => s.lastPromptAt)) : lane.lastActivity;
}

/** How long a worktree in Needs you has waited, as the engine has it (`lane_waiting_since`):
 *  since the longest wait of its own agents that need you and aren't snoozed. Null when it needs
 *  you for its PR alone, or only through snoozed agents. */
function laneWaitingSince(lane: Lane): number | null {
  const waits = lane.sessions.filter((s) => !s.visitor && !s.archived && !s.snoozedUntil).flatMap((s) => waitingSince(s) ?? []);
  return waits.length > 0 ? Math.min(...waits) : null;
}

/** Worktrees in a bay, as the engine sorts them (`lane_order`): in Needs you the longest wait
 *  first, then those with no agent waiting; within that, and in the other bays, the one you
 *  last sent a prompt first. */
function byLane(a: Lane, b: Lane): number {
  const [x, y] = a.bay === 'needs' ? [laneWaitingSince(a), laneWaitingSince(b)] : [null, null];
  const waited = x === y ? 0 : x === null ? 1 : y === null ? -1 : x - y;
  return waited || lanePromptAt(b) - lanePromptAt(a) || b.lastActivity - a.lastActivity;
}

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
 * `headline`), an agent bay takes its sentence from the first agent with that status in the
 * lane's order, the one you last sent a prompt; the rest get a plain one.
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

/** Text in backticks, as the engine's `code` writes it: shown as code. */
function code(s: string): string {
  return `\`${s.replace(/`/g, "'")}\``;
}

function truncate(s: string, max: number): string {
  return [...s].length <= max ? s : `${[...s].slice(0, max - 1).join('').trimEnd()}…`;
}

/** A session's own sentence, as the engine words it (board.rs, `session_headline`). */
export function sessionHeadline(s: Session): [string, string | null] {
  const acct = code(s.account);
  if (s.stalledSince != null) return [`${acct} is stuck · ${s.title}`, s.excerpt];
  switch (s.status) {
    case 'needs':
      if (s.excerpt == null) return [`${acct} needs you`, s.title];
      if (s.harness === 'claude') return [`${acct} is waiting for approval`, s.excerpt];
      if (s.harness === 'codex' || s.harness === 'opencode') return [`${acct} asks for approval`, s.excerpt];
      return [`${acct} needs you`, s.excerpt];
    case 'error':
      return [`${acct} stopped with an error`, s.excerpt];
    case 'working': {
      // How long it has worked on what you last asked, as the engine counts it.
      const since = s.statusSince != null ? Math.max(s.statusSince, s.lastPromptAt) : null;
      const detail = since != null ? (s.excerpt != null ? `{for:${since}} · ${s.excerpt}` : `{for:${since}}`) : s.excerpt;
      return [`${acct} · ${s.title}`, detail];
    }
    case 'unread':
      return [`${acct} finished · ${s.title}`, s.excerpt];
    case 'idle':
      return [`${acct} · ${s.title}`, `last turn {ago:${s.lastActivity}}`];
    case 'stopped':
      return [`${acct} · ${s.title}`, `stopped {ago:${s.lastActivity}}`];
  }
}

/** An idle lane's sentence, as the engine words it: its latest own session, else the session that
 *  made it or worked in it, else what git says. */
function idleHeadline(lane: Lane, own: Session[]): [string, string | null] {
  const latest = own.reduce<Session | null>((a, s) => (a == null || s.lastActivity > a.lastActivity ? s : a), null);
  if (latest) return [sessionHeadline(latest)[0], `last turn {ago:${latest.lastActivity}}`];
  const guest = lane.sessions.find((s) => s.visitor && !s.archived);
  if (guest) return [`${guest.visitor === 'made this worktree' ? 'Made by' : 'Worked on by'} ${code(guest.account)} · ${guest.title}`, `last turn {ago:${guest.lastActivity}}`];
  const g = lane.git;
  if (g && g.dirty > 0) return ['Uncommitted changes', `${g.dirty} files`];
  if (g?.lastCommit) {
    const commit = truncate(g.lastCommit, 70).replace(/`/g, "'");
    return ['No agents', g.lastCommitAt != null ? `Last commit {ago:${g.lastCommitAt}} · ${commit}` : commit];
  }
  return ['No agents', null];
}

/**
 * The lane with its sessions' statuses changed (src/ahead.ts): the bay, glyph and sentence
 * worked out again from its own sessions, as the engine would. A lane whose PR puts it in review
 * or merged keeps the engine's sentence when it was there already.
 */
export function relaned(lane: Lane, sessions: Session[]): Lane {
  const own = sessions.filter((s) => !s.visitor && !s.archived).sort(bySession);
  const [bay, glyph] = bayOf(own, lane.pr);
  const next = { ...lane, sessions, bay, glyph };
  if (bay === 'needs' || bay === 'working' || bay === 'unread') {
    const first = own.find((s) => s.status === glyph) ?? (bay === 'needs' ? own.find((s) => s.status === 'error') : undefined);
    if (first) return { ...next, headline: first.headline, detail: first.detail };
  }
  if (bay === 'idle') {
    const [headline, detail] = idleHeadline(lane, own);
    return { ...next, headline, detail };
  }
  if (bay === lane.bay) return next;
  const [headline, detail] = headlineFor(bay, glyph, own, lane.pr, '');
  return { ...next, headline, detail };
}

/** The lane with only `sessions`. When that changes its status, the bay, glyph and headline are worked out again. */
function withSessions(lane: Lane, sessions: Session[], since: string): Lane {
  if (sessions.length === lane.sessions.length) return lane;
  const own = sessions.filter((s) => !s.visitor);
  const [bay, glyph] = bayOf(own, lane.pr);
  if (bay === lane.bay && glyph === lane.glyph) return { ...lane, sessions };
  const [headline, detail] = headlineFor(bay, glyph, own, lane.pr, since);
  return { ...lane, sessions, bay, glyph, headline, detail };
}

/** The lane with only the sessions active since `cutoff`. */
export function ageFiltered(lane: Lane, cutoff: number, since: string): Lane {
  if (cutoff <= 0) return lane;
  return withSessions(
    lane,
    lane.sessions.filter((s) => s.lastActivity >= cutoff),
    since,
  );
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

/** A worktree's row: a line for each own agent that needs you, is working or just finished. */
export function worktreeRow(lane: Lane): WorktreeRow {
  const agents = lane.sessions
    .filter((s) => !s.visitor && !s.archived && AGENT_BAY[s.status])
    .sort(bySession)
    .map((session): AgentItem => ({ kind: 'agent', key: `${lane.id}::${session.key}`, lane, session }));
  return { key: lane.id, lane, agents };
}

/** What you can select in a row: its agents, or the worktree itself when it has none. */
export function rowItems(row: WorktreeRow): BoardRow[] {
  return row.agents.length > 0 ? row.agents : [{ kind: 'lane', key: row.lane.id, lane: row.lane }];
}

/**
 * Each worktree once, in the bay its status puts it in, with its agents on its row. In Needs
 * you the worktree that has waited longest comes first; in the other bays, the one you last
 * sent a prompt.
 */
export function buildBays(lanes: Lane[]): { bay: Bay; rows: WorktreeRow[] }[] {
  const groups: Record<Bay, Lane[]> = { needs: [], working: [], unread: [], review: [], idle: [], merged: [] };
  for (const lane of lanes) groups[lane.bay].push(lane);
  return BAYS.map((bay) => ({ bay, rows: groups[bay].sort(byLane).map(worktreeRow) })).filter((g) => g.rows.length > 0);
}

/**
 * A worktree as an Inbox filter shows it, or null when the filter leaves it out. Needs you,
 * Working and Your turn narrow it to its agents in that state, and it moves to that state's
 * bay, as it does under the age filter; In review and Checks failing are about its PR and keep
 * it whole.
 */
export function narrowed(lane: Lane, f: Filter | null, since: string): Lane | null {
  if (!f) return lane;
  if (f === 'failing') return failingPr(lane) ? lane : null;
  if (f !== 'needs' && f !== 'working' && f !== 'unread') return lane.bay === f ? lane : null;
  const sessions = lane.sessions.filter((s) => !s.visitor && AGENT_BAY[s.status] === f);
  // A worktree can need you for its PR alone: failing checks or changes requested.
  if (sessions.length === 0) return f === 'needs' && lane.bay === 'needs' ? { ...lane, sessions: [] } : null;
  // It shows in the filter's bay even when its agents alone would put it elsewhere: finished
  // agents on a branch whose checks fail would otherwise make it need you.
  const glyph: Status = f === 'needs' ? (sessions.some((s) => s.status === 'needs') ? 'needs' : 'error') : f;
  const [headline, detail] = headlineFor(f, glyph, sessions, lane.pr, since);
  return { ...lane, sessions, bay: f, glyph, headline, detail };
}

/**
 * The session that stands for a worktree that needs you for its PR alone (failing checks,
 * changes requested) where only sessions are listed: its latest own one, where you'd go back to
 * fix it, else the latest that worked there from another worktree (the one that made it, say).
 * A visitor that needs you is listed as that already. Null for any other worktree.
 */
export function prNeedsSession(lane: Lane): Session | null {
  if (lane.bay !== 'needs') return null;
  const own = lane.sessions.filter((s) => !s.visitor && !s.archived);
  if (own.some((s) => AGENT_BAY[s.status] === 'needs')) return null;
  const pool = own.length > 0 ? own : lane.sessions.filter((s) => s.visitor && !s.archived && AGENT_BAY[s.status] !== 'needs');
  return pool.reduce<Session | null>((a, s) => (a && a.lastActivity >= s.lastActivity ? a : s), null);
}

/**
 * The Inbox's counts: agents that need you, are working or just finished, and worktrees whose
 * PR needs you, is in review or has failing checks.
 */
export function inboxCounts(lanes: Lane[]): { needs: number; working: number; unread: number; review: number; failing: number } {
  const c = { needs: 0, working: 0, unread: 0, review: 0, failing: 0 };
  for (const lane of lanes) {
    const own = lane.sessions.filter((s) => !s.visitor);
    for (const s of own) {
      const bay = AGENT_BAY[s.status];
      if (bay === 'needs' || bay === 'working' || bay === 'unread') c[bay]++;
    }
    if (lane.bay === 'needs' && !own.some((s) => AGENT_BAY[s.status] === 'needs')) c.needs++;
    if (lane.bay === 'review') c.review++;
    if (failingPr(lane)) c.failing++;
  }
  return c;
}

/**
 * A session's sentence without the account it starts with, which the chip beside it shows:
 * "`claude` finished · Fix the test" reads "finished · Fix the test".
 */
export function withoutAccount(headline: string): string {
  return headline.replace(/^`[^`]*`\s*(?:·\s*)?/, '');
}
