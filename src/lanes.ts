// Lanes on the board: grouped into bays, seen through the age and status filters, and sorted.
// The engine sets a lane's bay and headline from all of its sessions; when a filter hides some
// of them, the lane's status and sentence are worked out again from what's left, with the same
// rules as the engine (board.rs, `bay_of` and `headline`). As there, a reusable worktree's
// sessions from before its latest work (`earlier`) are left out of its status, as archived ones
// are.

import { shortOf } from './devices';
import { withTimes } from './format';
import type { InsightLabel } from './insights';
import { resetsPhrase } from './format';
import type { Bay, Lane, Pr, PrItem, Session, SessionLimit, Status } from './types';

export const BAYS: Bay[] = ['needs', 'working', 'unread', 'review', 'idle', 'merged'];
/** The status filter an agent falls under, for the statuses that have one. */
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
/** A status the sidebar narrows the board to: agents in one state, or worktrees whose PR is in
 *  review or needs work. Several narrow it to what's in any of them; none, to nothing. */
export type Filter = Bay | 'work';

/** When a session began waiting on you, for one that needs you, as the engine has it
 *  (`waiting_since`): Needs you is a queue, the longest wait first, so a new one can't keep
 *  pushing an old one down. */
export function waitingSince(s: Session): number | null {
  return s.status === 'needs' || s.status === 'error' ? (s.statusSince ?? s.lastActivity) : null;
}

/** When a session's status began. */
const statusAt = (s: Session) => s.statusSince ?? s.lastActivity;

/**
 * Two sessions of one of the floating bar's groups, in its order (barfeed.rs): Needs you and
 * Working are queues, the longest first; Your turn reads as a feed, the newest status first. A
 * status's time doesn't move while it lasts, so rows hold their places.
 */
export function byStatusTime(a: Session, b: Session): number {
  const d = statusAt(a) - statusAt(b);
  return a.status === 'unread' ? -d : d;
}

/**
 * Most urgent first, as the engine sorts a lane's sessions; those that need you by how long
 * they've waited, the rest by the one you last sent a prompt. Activity would reorder working
 * agents with every tool call; your prompts only move one when you do something.
 */
export function bySession(a: Session, b: Session): number {
  return URGENCY[a.status] - URGENCY[b.status] || (waitingSince(a) ?? 0) - (waitingSince(b) ?? 0) || b.lastPromptAt - a.lastPromptAt || b.lastActivity - a.lastActivity;
}

/** When a session started, or when you last prompted it where its record doesn't say. */
export function startedOf(s: Session): number {
  return s.startedAt ?? s.lastPromptAt;
}

/** A finished turn waiting on you to read it: Your turn, or a reply that asks you something.
 *  Marking it seen makes it idle, which takes it off the bar and out of its bay. */
export function seenable(s: Pick<Session, 'status' | 'asks'>): boolean {
  return s.status === 'unread' || !!s.asks;
}

/** Whether Codealong can close a session from its menu (idle.rs `close_one`): one running on
 *  this device in a terminal of its own, whose tab closes, or as a background job, which stops.
 *  The apps' sessions and the ones Codealong runs itself have no terminal to close. */
/** Whether Codealong can stop the turn a session is taking. A session Codealong runs on a Mac
 *  from before stops reached every harness says so by working alone. */
export function stoppable(s: Session): boolean {
  return !!s.stoppable || (!!s.hosted && s.status === 'working');
}

/** The PRs a session worked on, newest activity first, as the PRs view links them (board.rs
 *  `pr_items`); else its worktree's PR, when the session is on the worktree's branch. A PR the
 *  board doesn't list (one on another device's repo) still comes from its worktree. */
export function sessionPrs(prs: PrItem[], lane: Lane, s: Session): Pr[] {
  const worked = prs.filter((p) => p.sessions.some((x) => x.key === s.key));
  if (worked.length > 0) return worked;
  return lane.pr && (s.branch ?? lane.branch) === lane.branch ? [lane.pr] : [];
}

export function closable(s: Session): boolean {
  return s.live && s.surface === 'cli' && !s.hosted && !shortOf(s.key) && (s.pid != null || s.attachCommand != null);
}

/** A session and the worktree it's listed under as its own, not one it only visits. */
export interface Placed {
  lane: Lane;
  session: Session;
}

/** Where each session lives on the board: the lane it's listed under as its own. */
export function sessionHomes(lanes: Lane[]): Map<string, Placed> {
  const m = new Map<string, Placed>();
  for (const lane of lanes) for (const session of lane.sessions) if (!session.visitor && !m.has(session.key)) m.set(session.key, { lane, session });
  return m;
}

/** Where one session lives on the board (`sessionHomes` for one), or null once it's gone. */
export function homeOf(lanes: Lane[], key: string | null | undefined): Placed | null {
  if (!key) return null;
  for (const lane of lanes) {
    const session = lane.sessions.find((s) => s.key === key && !s.visitor);
    if (session) return { lane, session };
  }
  return null;
}

/** The session to open for something with several (a todo, a PR): the running one, else the one
 *  most recently active. */
export function liveOrLatest<T extends { session: Session }>(list: T[]): T | null {
  const better = (a: Session, b: Session) => (a.live !== b.live ? a.live : a.lastActivity > b.lastActivity);
  return list.reduce<T | null>((best, x) => (!best || better(x.session, best.session) ? x : best), null);
}

/** A worktree's branch as the lists and panels name it: the branch, or the commit a detached
 *  HEAD is at; null when git hasn't said either. */
export function branchLabel(lane: Pick<Lane, 'branch' | 'detached'>): string | null {
  return lane.branch ?? (lane.detached ? `detached @ ${lane.detached}` : null);
}

/** A worktree's name beside its repo: `main` for the repo's main worktree, as git calls it, else
 *  its callsign. Where the repo isn't beside it, the callsign (the repo's own name, for the main
 *  worktree) says more. */
export function worktreeName(lane: Lane): string {
  return lane.origin === 'main' ? 'main' : lane.callsign;
}

/** When you last sent a prompt to one of a worktree's own agents; with none, when anything last happened in it. */
export function lanePromptAt(lane: Lane): number {
  const own = lane.sessions.filter((s) => !s.visitor && !s.archived);
  return own.length > 0 ? Math.max(...own.map((s) => s.lastPromptAt)) : lane.lastActivity;
}

/** How long a worktree in Needs you has waited, as the engine has it (`lane_waiting_since`):
 *  since the longest wait of its own agents that need you and aren't snoozed. Null when it needs
 *  you only through snoozed agents. */
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

function prOpen(pr: Pick<Pr, 'state'>): boolean {
  return pr.state !== 'merged' && pr.state !== 'closed';
}

/** The lane's open PR when its checks are failing. */
export function failingPr(lane: Lane): Pr | null {
  const pr = lane.pr;
  return pr && prOpen(pr) && pr.checks.failed > 0 ? pr : null;
}

/** Whether an open PR needs work: failing checks or changes requested. The sidebar's PR needs
 *  work, for worktrees, sessions, PRs and todos alike. */
export function prNeedsWork(pr: Pick<Pr, 'state' | 'checks'> | null | undefined): boolean {
  return !!pr && prOpen(pr) && (pr.checks.failed > 0 || pr.state === 'changes');
}

/** The first rule that matches, as in the engine: agents that need you, work, finish, then the
 *  PR. A PR that needs work doesn't make the worktree need you; PR needs work finds it. */
export function bayOf(sessions: Session[], pr: Pr | null): [Bay, Status] {
  const has = (st: Status) => sessions.some((s) => s.status === st);
  if (has('needs')) return ['needs', 'needs'];
  if (has('error')) return ['needs', 'error'];
  if (has('working')) return ['working', 'working'];
  if (has('unread')) return ['unread', 'unread'];
  if (pr) return prOpen(pr) ? ['review', 'idle'] : ['merged', 'idle'];
  return ['idle', 'idle'];
}

/**
 * The sentence for a lane whose status changed under the filter. As in the engine (board.rs,
 * `headline`), an agent bay takes its sentence from the first agent with that status in the
 * lane's order, the one you last sent a prompt; the rest get a plain one.
 */
function headlineFor(bay: Bay, own: Session[], pr: Pr | null, since: string): [string, string | null] {
  const first = (...statuses: Status[]) => statuses.map((st) => own.find((s) => s.status === st)).find((s) => s != null);
  const agent = bay === 'needs' ? first('needs', 'error') : bay === 'working' || bay === 'unread' ? first(bay) : undefined;
  if (agent) return [agent.headline, agent.detail];
  if (pr && bay === 'merged') return [`#${pr.number} ${pr.state === 'closed' ? 'closed' : 'merged'}`, null];
  if (pr && bay === 'review') return [pr.checks.failed > 0 ? `#${pr.number} · checks failing` : `#${pr.number} ${pr.title}`, null];
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
  // Every headline names the session: its title after what it's doing.
  switch (s.status) {
    case 'needs':
      // A question, from its reply or held through the plugin, leaves the question itself below.
      if (s.asks || s.excerpt?.startsWith('Asks you')) return [`${acct} asks you · ${s.title}`, s.excerpt?.startsWith('Asks you · ') ? s.excerpt.slice('Asks you · '.length) : null];
      if (s.excerpt == null) return [`${acct} needs you · ${s.title}`, null];
      if (s.harness === 'claude') return [`${acct} is waiting for approval · ${s.title}`, s.excerpt];
      return [`${acct} asks for approval · ${s.title}`, s.excerpt];
    case 'error':
      if (s.limit) return [`${acct} hit its usage limit · ${s.title}`, s.excerpt];
      return [`${acct} stopped with an error · ${s.title}`, s.excerpt];
    case 'working': {
      // How long it has worked on what you last asked, as the engine counts it.
      const since = s.statusSince != null ? Math.max(s.statusSince, s.lastPromptAt) : null;
      const detail = since != null ? (s.excerpt != null ? `{for:${since}} · ${s.excerpt}` : `{for:${since}}`) : s.excerpt;
      return [`${acct} · ${s.title}`, detail];
    }
    case 'unread':
      // Its glyph and bay already say it finished, so the title stands alone, as a working one's does.
      return [`${acct} · ${s.title}`, s.excerpt];
    case 'idle':
      return [`${acct} · ${s.title}`, `last turn {ago:${s.lastActivity}}`];
    case 'stopped':
      return [`${acct} · ${s.title}`, `stopped {ago:${s.lastActivity}}`];
  }
}

/** The words of a meter's key: `session` is the 5-hour limit, `model:Opus` Opus's own week. */
function limitName(meter: string | null): string {
  if (meter === 'session') return '5-hour limit';
  if (meter === 'week') return 'weekly limit';
  if (meter?.startsWith('model:')) return `weekly ${meter.slice('model:'.length)} limit`;
  return 'usage limit';
}

/** What a usage limit that stopped a session comes to, in a sentence or two: which limit, when it
 *  lifts, and where it could go on (left out where a button beside it offers the move). */
export function limitWords(account: string, limit: SessionLimit, now: number, offered = false): string {
  const lifted = limit.resetsAt != null && limit.resetsAt <= now;
  const spent = limit.spendCapped ? ', with no extra usage to carry it' : '';
  const when = lifted ? ' It has lifted since: send it something to go on.' : limit.resetsAt != null ? ` It lifts ${resetsPhrase(limit.resetsAt, now)}.` : '';
  const where = lifted || offered ? '' : limit.moveTo ? ` ${limit.moveTo} has room: move it there to go on now.` : ' No other account has room.';
  return `It stopped at ${account}’s ${limitName(limit.meter)}${spent}.${when}${where}`;
}

/**
 * What a session waits on you for, as a label beside its title: the words its headline puts
 * before the title. A session's row leads with its title, and its glyph and bay already say it
 * needs you, so only the kind of need gets a label (none for a need with nothing more to say).
 */
export function sessionFlag(s: Session): InsightLabel | null {
  if (s.stalledSince != null) return { label: 'Stuck', tone: 'warn', title: 'It says it’s busy, but nothing has happened for a while. It may be waiting on something.' };
  if (s.limit && s.status !== 'working') return { label: 'Limit', tone: 'bad', title: limitWords(s.account, s.limit, Date.now()) };
  if (s.status === 'error') return { label: 'Error', tone: 'bad', title: 'It stopped with an error.' };
  if (s.status !== 'needs') return null;
  if (s.asks) return { label: 'Asks you', tone: 'warn', title: 'Its last reply asks you something.' };
  if (s.excerpt?.startsWith('Asks you')) return { label: 'Asks you', tone: 'warn', title: 'It’s waiting for your answer to its question.' };
  if (s.excerpt != null) return { label: 'Needs approval', tone: 'warn', title: 'It’s waiting for you to allow what it wants to run.' };
  return null;
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
  const own = sessions.filter((s) => !s.visitor && !s.archived && !s.earlier).sort(bySession);
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
  const [headline, detail] = headlineFor(bay, own, lane.pr, '');
  return { ...next, headline, detail };
}

/** The lane with only `sessions`. When that changes its status, the bay, glyph and headline are worked out again. */
function withSessions(lane: Lane, sessions: Session[], since: string): Lane {
  if (sessions.length === lane.sessions.length) return lane;
  const own = sessions.filter((s) => !s.visitor && !s.earlier);
  const [bay, glyph] = bayOf(own, lane.pr);
  if (bay === lane.bay && glyph === lane.glyph) return { ...lane, sessions };
  const [headline, detail] = headlineFor(bay, own, lane.pr, since);
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
    .filter((s) => !s.visitor && !s.archived && !s.earlier && AGENT_BAY[s.status])
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
 * A worktree as the status filters show it, or null when they leave it out. In review and PR
 * needs work are about its PR and keep it whole. Needs you, Working and Your turn narrow it to
 * its agents in those states, and it moves to the bay they put it in, as it does under the age
 * filter. No filter keeps every worktree.
 */
export function narrowed(lane: Lane, fs: readonly Filter[], since: string): Lane | null {
  if (fs.length === 0) return lane;
  const forPr = fs.some((f) => (f === 'work' ? prNeedsWork(lane.pr) : !AGENT_FILTERS.has(f) && lane.bay === f));
  if (forPr) return lane;
  const sessions = lane.sessions.filter((s) => !s.visitor && !s.earlier && fs.includes(AGENT_BAY[s.status] as Filter));
  if (sessions.length === 0) return null;
  // It shows in its agents' bay even when its other agents put it elsewhere: one working and one
  // finished shows under Your turn when that's what you picked.
  const [bay, glyph] = bayOf(sessions, lane.pr);
  const [headline, detail] = headlineFor(bay, sessions, lane.pr, since);
  return { ...lane, sessions, bay, glyph, headline, detail };
}

/** The statuses that are about agents, not a worktree's PR. */
const AGENT_FILTERS = new Set<Filter>(['needs', 'working', 'unread']);

/**
 * The status filters' counts: agents that need you, are working or just finished, and worktrees whose
 * PR is in review or needs work.
 */
export function inboxCounts(lanes: Lane[]): { needs: number; working: number; unread: number; review: number; work: number } {
  const c = { needs: 0, working: 0, unread: 0, review: 0, work: 0 };
  for (const lane of lanes) {
    const own = lane.sessions.filter((s) => !s.visitor && !s.earlier);
    for (const s of own) {
      const bay = AGENT_BAY[s.status];
      if (bay === 'needs' || bay === 'working' || bay === 'unread') c[bay]++;
    }
    if (lane.bay === 'review') c.review++;
    if (prNeedsWork(lane.pr)) c.work++;
  }
  return c;
}

/**
 * A session's sentence without the account it starts with, which the chip beside it shows:
 * "`claude` asks you · Fix the test" reads "asks you · Fix the test".
 */
export function withoutAccount(headline: string): string {
  return headline.replace(/^`[^`]*`\s*(?:·\s*)?/, '');
}

/** What a session is doing or said last, for a row under its title. Its time is in the row's
 *  own column, so the detail's running time is left out. */
export function sessionWhy(detail: string | null, headline: string, now: number): string {
  const text = detail ? detail.replace(/^\{for:-?\d+\}( · )?/, '') : withoutAccount(headline);
  return withTimes(text.replace(/`/g, ''), now);
}
