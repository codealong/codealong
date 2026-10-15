// Lanes on the board: grouped into bays, seen through the age and status filters, and sorted.
// The engine sets a lane's bay and headline from all of its sessions; when a filter hides some
// of them, the lane's status and sentence are worked out again from what's left, with the same
// rules as the engine (board.rs, `bay_of` and `headline`). As there, a reusable worktree's
// sessions from before its latest work (`earlier`) are left out of its status, as archived ones
// are.

import { shortOf } from './devices';
import { MOVES_ACCOUNTS } from './harness';
import { agoPhrase, duration, withTimes } from './format';
import type { InsightLabel } from './insights';
import { resetsPhrase } from './format';
import type { Bay, ConnectionLost, Lane, Pr, PrItem, Session, SessionLimit, SideGlance, Status } from './types';

export const BAYS: Bay[] = ['needs', 'working', 'unread', 'review', 'idle', 'merged'];
/** The status filter an agent falls under, for the statuses that have one. */
export const AGENT_BAY: Partial<Record<Status, Bay>> = { needs: 'needs', error: 'needs', working: 'working', unread: 'unread' };
/** How urgent a status is, as the engine ranks it. Idle and stopped rank the same: a Codex
 *  thread is never stopped, so telling them apart would only sort the harnesses apart. */
const URGENCY: Record<Status, number> = { needs: 0, error: 1, working: 2, unread: 3, idle: 4, stopped: 4 };

/** An agent in the worktree it runs in: a line in a worktree's row, or a row in Sessions. */
export type AgentItem = { kind: 'agent'; key: string; lane: Lane; session: Session };
/** A worktree no agent has worked in, which its row selects whole. */
export type LaneItem = { kind: 'lane'; key: string; lane: Lane };
/** Something you can select on the Worktrees board or in Sessions. */
export type BoardRow = AgentItem | LaneItem;
/**
 * A worktree's row on the board: the worktree, and a line for each of its agents that needs you,
 * is working or just finished; with none of those, a line for the agent that last worked in it.
 * Every line is a session, however busy the worktree is: what belongs to the worktree itself (its
 * PR, its app, its todo) is beside its name, never a line of its own.
 */
export interface WorktreeRow {
  key: string;
  lane: Lane;
  /** Empty only for a worktree no agent has worked in. */
  lines: AgentItem[];
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

/** A helper or a headless run that doesn't need you: what started it waits on it, not you, so
 *  it's out of its worktree's status, the bar, the mark and the queues, and Sessions lists it
 *  under the session it works for (`workingFor`), or counts it there. One that needs you (a call
 *  to allow, a question, an error) waits on you as any session does. The engine's
 *  `Session::unattended` says the same. */
export function unattended(s: Pick<Session, 'helper' | 'headless' | 'status'>): boolean {
  return (s.helper != null || !!s.headless) && s.status !== 'needs' && s.status !== 'error';
}

/** The session a helper or a headless run works for, by its key: the one whose agent started it
 *  as a helper, or the one a headless run came from, which for one Codealong saw running is the
 *  session whose command ran it (board.rs `started_by`). */
export function workingFor(s: Pick<Session, 'helper' | 'headless' | 'origin'>): string | undefined {
  return s.helper?.parent ?? (s.headless ? s.origin?.key : undefined);
}

/** What Sessions keeps to list every headless run of a session under it (boardLists.ts
 *  `placeHelpers`'s `shown`), beside the session's own key for its helpers: a judge's runs come
 *  by the hundred, so listing its helpers doesn't list them too. */
export function runsShownKey(parent: string): string {
  return `runs:${parent}`;
}

/** What Sessions keeps to list every side conversation of a session under it, answered ones too
 *  (`sidesListed`), beside its key for its helpers and its `runsShownKey`. */
export function sidesShownKey(parent: string): string {
  return `sides:${parent}`;
}

/** What works for a session that Sessions can list in full under it: its helpers, the headless
 *  runs its commands started, its side conversations. */
export type KidKind = 'helpers' | 'runs' | 'sides';

/** What `shown` holds to list every one of `kind` under the session `key`. */
export function shownKeyOf(kind: KidKind, key: string): string {
  return kind === 'helpers' ? key : kind === 'runs' ? runsShownKey(key) : sidesShownKey(key);
}

/** A way back from listing every one of a kind under a session (`shown`), at the end of what's
 *  listed under it: which kind, whose (`key`, `title`), and how many would leave the list. */
export interface Fold {
  kind: KidKind;
  key: string;
  title: string;
  hides: number;
}

/** Each one of a kind that works for a session, as its chip counts them (boardLists.ts
 *  `HelperCount`): where each stands. */
export interface KidCount {
  count: number;
  each: readonly { status: Status }[];
}

/** Whether one of a kind stays listed under its session without asking for them all: a helper
 *  at work, waiting on you or with a turn to see (`placeHelpers`), a run waiting on you
 *  (`unattended`), the side conversation answering (`sidesListed`). */
function staysListed(kind: KidKind, status: Status): boolean {
  return kind === 'helpers' ? AGENT_BAY[status] != null : kind === 'runs' ? status === 'needs' || status === 'error' : status === 'working';
}

/** The folds for the sessions of one family Sessions lists (`members`: the session at its head,
 *  then the ones under it), each kind listed in full for one of them that has any, with how many
 *  of them would leave the list (`staysListed`). */
export function foldsOf(members: readonly Pick<Session, 'key' | 'title'>[], shown: ReadonlySet<string>, counts: Record<KidKind, (key: string) => KidCount | undefined>): Fold[] {
  if (shown.size === 0) return [];
  const out: Fold[] = [];
  for (const m of members) {
    for (const kind of ['helpers', 'runs', 'sides'] as const) {
      const c = counts[kind](m.key);
      if (!c || c.count === 0 || !shown.has(shownKeyOf(kind, m.key))) continue;
      out.push({ kind, key: m.key, title: m.title, hides: c.each.filter((g) => !staysListed(kind, g.status)).length });
    }
  }
  return out;
}

/** The side conversations Sessions lists under a session (`key`): the one answering, as a helper
 *  at work is, or every one where you asked (`sidesShownKey` in `shown`, its chip's double-click),
 *  the latest started first. They're questions about the session, not sessions: a row's click
 *  shows its conversation above the session's box. */
export function sidesListed(list: readonly SideGlance[] | undefined, key: string, shown: ReadonlySet<string>): readonly SideGlance[] {
  if (!list || list.length === 0) return NO_SIDES;
  return shown.has(sidesShownKey(key)) ? list : list.filter((g) => g.running);
}
const NO_SIDES: readonly SideGlance[] = [];

/** What a fold says: what leaves the list, and whose when it isn't the family's head. */
export function foldWords(f: Fold, head: boolean): string {
  const many = (n: number, one: string) => `${n} ${one}${n === 1 ? '' : 's'}`;
  const what =
    f.kind === 'helpers'
      ? f.hides > 0
        ? `Hide ${many(f.hides, 'finished helper')}`
        : 'List only the helpers at work'
      : f.kind === 'runs'
        ? f.hides > 0
          ? `Hide ${many(f.hides, 'headless run')}`
          : 'List none of the headless runs'
        : f.hides > 0
          ? `Hide ${many(f.hides, 'answered side conversation')}`
          : 'List only the side conversation answering';
  return head ? what : `${what} of ${f.title}`;
}

/** What a side conversation is called in a list: what it was first asked. */
export function sideTitle(g: Pick<SideGlance, 'question'>): string {
  return g.question || 'A side conversation';
}

/**
 * Two sessions of one of the floating bar's groups, in its order (barfeed.rs): each is a queue,
 * the longest first, so a new one joins the end, and what waits on you is worked through first
 * come, first served (queue.ts `byPlace`). A status's time doesn't move while it lasts, so rows
 * hold their places.
 */
export function byStatusTime(a: Session, b: Session): number {
  return statusAt(a) - statusAt(b);
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
export function seenable(s: Pick<Session, 'status' | 'asks' | 'cutOff'>): boolean {
  return s.status === 'unread' || !!s.asks || (s.status === 'error' && s.cutOff != null);
}

/** Whether a session can be marked unread: its turn is over and you've seen it (idle or
 *  stopped), and it isn't archived. It's Your turn again until it's seen or prompted (board.rs
 *  `apply_unread`). */
export function unreadable(s: Pick<Session, 'status' | 'asks' | 'archived'>): boolean {
  return !s.archived && !s.asks && (s.status === 'idle' || s.status === 'stopped');
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

/** Whether Mark as done ends it by its harness's own exit, typed into its terminal, which cleans
 *  up the worktree the harness made for it, rather than closing its tab: idle.rs
 *  `exits_its_own_way` says the same. Not while that exit would be taken for something else: a
 *  turn running, a command left in the background, or a prompt waiting on you. */
export function exitsItsOwnWay(s: Session): boolean {
  return s.ownWorktree != null && closable(s) && s.attachCommand == null && s.status !== 'working' && s.background == null && !(s.status === 'needs' && !s.asks);
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
  // A helper's prompts are its agent's, not yours, and a headless run's its script's.
  const own = lane.sessions.filter((s) => !s.visitor && !s.archived && !unattended(s));
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
  // In the engine's order, whatever order they came in: a lane changed ahead of the engine
  // (ahead.ts) keeps its sessions where they were.
  const ordered = [...own].sort(bySession);
  const first = (...statuses: Status[]) => statuses.map((st) => ordered.find((s) => s.status === st)).find((s) => s != null);
  const agent = bay === 'needs' ? first('needs', 'error') : bay === 'working' || bay === 'unread' ? first(bay) : undefined;
  if (agent) return [agent.headline, agent.detail];
  if (pr && bay === 'merged') return [`#${pr.number} ${pr.state === 'closed' ? 'closed' : 'merged'}`, null];
  if (pr && bay === 'review') return [pr.checks.failed > 0 ? `#${pr.number} · checks failing` : `#${pr.number} ${pr.title}`, null];
  return [`No agents active ${since}`, null];
}

/** One helper as its session's row shows it: a status glyph on the chip, and a row on the chip's
 *  card (HelpersCard.tsx). Values, not the session, so the row draws again only when one of them
 *  changes (memoRow.ts). */
export interface HelperGlance {
  key: string;
  title: string;
  status: Status;
  /** What it left running in the background, which its glyph draws as waiting. */
  background?: string;
  /** When its status began. */
  since: number;
  /** The PR it reviews (`#204`), for an automatic review. */
  review: string | null;
  /** What it's doing, asking you, or last said (`Session.excerpt`). */
  excerpt: string | null;
  /** Who runs it, on which model when that's known. */
  account: string;
  model: string | null;
  /** Its worktree, where it isn't its session's. */
  worktree: string | null;
  /** Its session is told when its turn ends (`codealong start --helper --report`). */
  reports: boolean;
}

/** What a helper is called on its session's chip: an automatic review by the PR it reviews. */
export function helperName(g: Pick<HelperGlance, 'title' | 'review'>): string {
  return g.review ? `Review of ${g.review}` : g.title;
}

/** Where a helper stands, since when: `working for 3m`, `done 12m ago`. */
export function helperStands(g: Pick<HelperGlance, 'status' | 'since'>, now: number): string {
  return g.status === 'working'
    ? `working for ${duration(g.since, now)}`
    : g.status === 'needs'
      ? `waits on you, ${agoPhrase(g.since, now)}`
      : g.status === 'error'
        ? `stopped with an error ${agoPhrase(g.since, now)}`
        : `done ${agoPhrase(g.since, now)}`;
}

/** A helper's line in its session's chip's tooltip, where the chip has no card: what it is, and
 *  where it stands since when. */
export function helperLine(g: Pick<HelperGlance, 'title' | 'review' | 'status' | 'since'>, now: number): string {
  return `${helperName(g)} · ${helperStands(g, now)}`;
}

/** Text in backticks, as the engine's `code` writes it: shown as code. */
function code(s: string): string {
  return `\`${s.replace(/`/g, "'")}\``;
}

function truncate(s: string, max: number): string {
  return [...s].length <= max ? s : `${[...s].slice(0, max - 1).join('').trimEnd()}…`;
}

/** How a session's excerpt starts while it holds another session's message for you to allow,
 *  as the engine writes it (board.rs `HELD_MESSAGE`). */
const HELD_MESSAGE = 'Held message from ';

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
      // Another session's message it holds for you to allow (claude/held.rs): who it's from below.
      if (s.excerpt.startsWith(HELD_MESSAGE)) return [`${acct} holds a message for you · ${s.title}`, s.excerpt.slice('Held message '.length)];
      if (s.harness === 'claude') return [`${acct} is waiting for approval · ${s.title}`, s.excerpt];
      return [`${acct} asks for approval · ${s.title}`, s.excerpt];
    case 'error':
      if (s.limit) return [`${acct} hit its usage limit · ${s.title}`, s.excerpt];
      if (s.cutOff != null) return [`${acct} was cut off · ${s.title}`, s.excerpt];
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
 *  lifts, and where it could go on (left out where a button beside it offers the move, and where
 *  its harness can't move a session to another account: `moves`, `MOVES_ACCOUNTS`). */
export function limitWords(account: string, limit: SessionLimit, now: number, offered = false, moves = true): string {
  const lifted = limit.resetsAt != null && limit.resetsAt <= now;
  const spent = limit.spendCapped ? ', with no extra usage to carry it' : '';
  const when = lifted ? ' It has lifted since: send it something to go on.' : limit.resetsAt != null ? ` It lifts ${resetsPhrase(limit.resetsAt, now)}.` : '';
  const where = lifted || offered || !moves ? '' : limit.moveTo ? ` ${limit.moveTo} has room: move it there to go on now.` : ' No other account has room.';
  return `It stopped at ${account}’s ${limitName(limit.meter)}${spent}.${when}${where}`;
}

/** What a lost connection that ended a session's turn comes to, and who carries it on. */
export function connectionLostWords(lost: ConnectionLost): string {
  const then = lost.continues ? 'Codealong tells it to carry on once the API can be reached again.' : 'Send it something to carry on.';
  return `Its last turn ended because it couldn’t reach its API: the Mac slept, or the network dropped. ${then}`;
}

/** What a cut-off turn comes to (`Session.cutOff`), and how it goes on (left out where a button
 *  beside it offers that). */
export function cutOffWords(offered = false): string {
  const then = offered ? '' : ' Resume it, or send it something, to carry on.';
  return `Its last turn was cut off partway: what ran it ended (its terminal closed, Codealong or the Mac restarted, or it was killed).${then}`;
}

/**
 * What a session waits on you for, as a label beside its title: the words its headline puts
 * before the title. A session's row leads with its title, and its glyph and bay already say it
 * needs you, so only the kind of need gets a label (none for a need with nothing more to say).
 */
export function sessionFlag(s: Session): InsightLabel | null {
  if (s.stalledSince != null) return { label: 'Stuck', tone: 'warn', title: 'It says it’s busy, but nothing has happened for a while. It may be waiting on something.' };
  if (s.limit && s.status !== 'working') return { label: 'Limit', tone: 'bad', title: limitWords(s.account, s.limit, Date.now(), false, MOVES_ACCOUNTS.includes(s.harness)) };
  if (s.connectionLost && s.status !== 'working') return { label: 'Network error', tone: 'warn', title: connectionLostWords(s.connectionLost) };
  if (s.cutOff != null && s.status !== 'working') return { label: 'Cut off', tone: 'bad', title: cutOffWords() };
  if (s.status === 'error') return { label: 'Error', tone: 'bad', title: 'It stopped with an error.' };
  if (s.status !== 'needs') return null;
  if (s.asks) return { label: 'Asks you', tone: 'warn', title: 'Its last reply asks you something.' };
  if (s.excerpt?.startsWith('Asks you')) return { label: 'Asks you', tone: 'warn', title: 'It’s waiting for your answer to its question.' };
  if (s.excerpt?.startsWith(HELD_MESSAGE)) return { label: 'Message', tone: 'warn', title: 'Another session sent it a message, which it holds until you deliver or deny it.' };
  if (s.excerpt != null) return { label: 'Needs approval', tone: 'warn', title: 'It’s waiting for you to allow what it wants to run.' };
  return null;
}

/** An idle lane's sentence, as the engine words it: its latest own session, else the session that
 *  made it or worked in it, else what git says. */
function idleHeadline(lane: Lane, own: Session[]): [string, string | null] {
  const latest = latestOf(own);
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

/** Whether a session counts toward its worktree's status, as board.rs has it: the worktree's own
 *  (not a visitor's), not archived, not a reusable worktree's earlier work, and not a helper or
 *  headless run that doesn't need you. The bays, the rows' lines, the status filters and their
 *  counts all go by it. */
function inStatus(s: Session): boolean {
  return !s.visitor && !s.archived && !s.earlier && !unattended(s);
}

/**
 * The lane with its sessions' statuses changed (src/ahead.ts): the bay, glyph and sentence
 * worked out again from its own sessions, as the engine would. A lane whose PR puts it in review
 * or merged keeps the engine's sentence when it was there already.
 */
export function relaned(lane: Lane, sessions: Session[]): Lane {
  const own = sessions.filter(inStatus).sort(bySession);
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

/**
 * The lane with only `sessions`, its status worked out again from its own current ones, as the
 * engine's is (archived ones and a reusable worktree's earlier work left out). An agent bay's
 * sentence is always worked out again: it's the first agent's of that status (board.rs
 * `headline`), and the one the engine named may be one the filter took out, with another of the
 * same status left. A PR's or an idle worktree's sentence stays the engine's while its bay does.
 */
function withSessions(lane: Lane, sessions: Session[], since: string): Lane {
  if (sessions.length === lane.sessions.length) return lane;
  const own = sessions.filter(inStatus);
  const [bay, glyph] = bayOf(own, lane.pr);
  const agents = bay === 'needs' || bay === 'working' || bay === 'unread';
  if (!agents && bay === lane.bay && glyph === lane.glyph) return { ...lane, sessions };
  const [headline, detail] = headlineFor(bay, own, lane.pr, since);
  return { ...lane, sessions, bay, glyph, headline, detail };
}

/** The copies of worktrees one of the lists makes, each with what it was made for. */
export type LaneCopies = WeakMap<Lane, { args: string; copy: Lane }>;

/**
 * The copy of `lane` that `make` made for `args` the last time it was asked, while `lane` is the
 * same object and `args` the same; else a new one, kept in `copies`. A push hands back every
 * worktree that didn't change as the same object (reuse.ts), so its copy stays the same too, and
 * its row skips rendering (memoRow.ts) where a fresh copy would render it again on every push.
 */
export function keptCopy(lane: Lane, copies: LaneCopies, args: string, make: () => Lane): Lane {
  const kept = copies.get(lane);
  if (kept && kept.args === args) return kept.copy;
  const copy = make();
  copies.set(lane, { args, copy });
  return copy;
}
const AGED: LaneCopies = new WeakMap();

/** The lane with only the sessions active since `cutoff`. */
export function ageFiltered(lane: Lane, cutoff: number, since: string): Lane {
  if (cutoff <= 0) return lane;
  return keptCopy(lane, AGED, `${cutoff} ${since}`, () =>
    withSessions(
      lane,
      lane.sessions.filter((s) => s.lastActivity >= cutoff),
      since,
    ),
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

/** The one most recently active. */
function latestOf(sessions: Session[]): Session | null {
  return sessions.reduce<Session | null>((a, s) => (a == null || s.lastActivity > a.lastActivity ? s : a), null);
}

/**
 * A worktree's row: a line for each own agent that needs you, is working or just finished, in
 * the engine's order. With none, one quiet line for the agent that last worked in it, as the
 * engine's sentence for an idle worktree names it (board.rs `headline`): its own latest (not
 * archived, nor a reusable worktree's earlier work), else the latest that made it or worked in it.
 */
export function worktreeRow(lane: Lane): WorktreeRow {
  const own = lane.sessions.filter(inStatus);
  const agents = own.filter((s) => AGENT_BAY[s.status]).sort(bySession);
  const quiet = agents.length > 0 ? null : (latestOf(own) ?? latestOf(lane.sessions.filter((s) => s.visitor && !s.archived)));
  const shown = agents.length > 0 ? agents : quiet ? [quiet] : [];
  return { key: lane.id, lane, lines: shown.map((session): AgentItem => ({ kind: 'agent', key: `${lane.id}::${session.key}`, lane, session })) };
}

/** What you can select in a row: its lines, or the worktree itself when no agent has worked in it. */
export function rowItems(row: WorktreeRow): BoardRow[] {
  return row.lines.length > 0 ? row.lines : [{ kind: 'lane', key: row.lane.id, lane: row.lane }];
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
 * its agents in those states (`inStatus`: not archived ones), and it moves to the bay they put it
 * in, as it does under the age filter. No filter keeps every worktree.
 */
export function narrowed(lane: Lane, fs: readonly Filter[], since: string): Lane | null {
  if (fs.length === 0) return lane;
  const forPr = fs.some((f) => (f === 'work' ? prNeedsWork(lane.pr) : !AGENT_FILTERS.has(f) && lane.bay === f));
  if (forPr) return lane;
  const sessions = lane.sessions.filter((s) => inStatus(s) && fs.includes(AGENT_BAY[s.status] as Filter));
  if (sessions.length === 0) return null;
  // The same copy while the worktree and the filters stay the same, so its row skips rendering
  // between pushes, as `ageFiltered`'s does (`keptCopy`).
  return keptCopy(lane, NARROWED, `${[...fs].sort().join(',')} ${since}`, () => {
    // It shows in its agents' bay even when its other agents put it elsewhere: one working and
    // one finished shows under Your turn when that's what you picked.
    const [bay, glyph] = bayOf(sessions, lane.pr);
    const [headline, detail] = headlineFor(bay, sessions, lane.pr, since);
    return { ...lane, sessions, bay, glyph, headline, detail };
  });
}
const NARROWED: LaneCopies = new WeakMap();

/** The statuses that are about agents, not a worktree's PR. */
const AGENT_FILTERS = new Set<Filter>(['needs', 'working', 'unread']);

/**
 * The status filters' counts: agents that need you, are working or just finished (`inStatus`), and
 * worktrees whose PR is in review or needs work.
 */
export function inboxCounts(lanes: Lane[]): { needs: number; working: number; unread: number; review: number; work: number } {
  const c = { needs: 0, working: 0, unread: 0, review: 0, work: 0 };
  for (const lane of lanes) {
    for (const s of lane.sessions.filter(inStatus)) {
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

/** Working only in that it waits on a command its turn left running in the background (a CI
 *  watch, a build), which wakes it when it ends: counted as working, drawn with a clock rather
 *  than a spinner. The engine's `Session::waits_in_background` says the same. */
export function waitsInBackground(s: Pick<Session, 'status' | 'background'>): boolean {
  return s.status === 'working' && s.background != null;
}

/** A worktree that's working only through sessions that wait on their background commands. */
export function laneWaits(lane: Pick<Lane, 'glyph' | 'sessions'>): boolean {
  const working = lane.sessions.filter((s) => s.status === 'working');
  return lane.glyph === 'working' && working.length > 0 && working.every(waitsInBackground);
}
