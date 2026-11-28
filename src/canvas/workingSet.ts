// Dynamic Working Set mode for Canvas (ui/WorkingSetCanvas.tsx): a self-curating, session-first
// spatial map that cuts a project's full session/schedule history down to the live working set,
// absorbs one-off follow-up schedules into session timer chips, collapses routine recurring runs
// into schedule hubs, extracts real relatedness edges (helpers, origins, peers, schedule runs,
// cross-referenced session IDs & PRs, non-main worktrees, and lightweight TF-IDF topic affinity),
// forms auto-named neighborhoods, and settles everything with a deterministic damped force +
// overlap-free rectangular packing layout that respects user-pinned positions.

import { takesAll, type BoardDoc } from '../boards/model';
import { duration, resetsPhrase } from '../format';
import { sessionPrs, type Placed } from '../lanes';
import { prReady } from '../prStatus';
import { runsOnce } from '../schedules';
import type { Label, LabelColor, Lane, Pr, PrItem, ScheduleItem, Session, Todo } from '../types';
import { sessionLine } from '../ui/card/data';
import { refId, type Entities } from './model';
import { hashStr, layoutWorkingSet, WS_CARD_H, WS_CARD_W, WS_HUB_H, WS_HUB_W } from './workingSetLayout';

export { WS_CARD_H, WS_CARD_W, WS_HUB_H, WS_HUB_W };
const DAY_MS = 86_400_000;

export type ActionTier = 'urgent' | 'handoff' | 'decision' | 'working' | 'quiet';

export interface ActionBadge {
  tier: ActionTier;
  label: string;
  deadline?: string;
  /** Lower is more urgent (0 urgent … 4 quiet). */
  rank: number;
}

export interface SessionTimer {
  scheduleId: string;
  name: string;
  at: number;
  queued: boolean;
  words: string;
}

export type WorkingEdgeKind = 'helper' | 'origin' | 'schedule' | 'mention' | 'pr' | 'peer' | 'worktree' | 'topic';

export interface WorkingEdge {
  id: string;
  from: string;
  to: string;
  kind: WorkingEdgeKind;
  weight: number;
  label?: string;
  crossCluster?: boolean;
}

export interface SessionNode {
  kind: 'session';
  id: string;
  key: string;
  session: Session;
  lane: Lane;
  action: ActionBadge;
  summary: string;
  timers: SessionTimer[];
  helpers: Placed[];
  prs: Pr[];
  scheduleName?: string;
  pinned: boolean;
}

export interface ScheduleHubNode {
  kind: 'schedule';
  id: string;
  schedule: ScheduleItem;
  quietRuns: number;
  totalRuns: number;
  nextLabel: string;
  latestRunKey?: string;
  action: ActionBadge;
  pinned: boolean;
}

export type WorkingNode = SessionNode | ScheduleHubNode;

export interface WorkingCluster {
  id: string;
  title: string;
  color: LabelColor;
  nodeIds: string[];
  x: number;
  y: number;
  w: number;
  h: number;
  urgentCount: number;
  actionCount: number;
  workingCount: number;
  custom: boolean;
}

export interface NodeBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** User overrides kept per project in stored.ts: pinned positions, shelved/pulled-in items,
 *  manual cluster groups and custom cluster titles. */
export interface WorkingSetPrefs {
  pinnedPos: Record<string, { x: number; y: number }>;
  shelved: string[];
  pinnedIn: string[];
  groups: Record<string, string>;
  groupNames: Record<string, string>;
}

export function emptyPrefs(): WorkingSetPrefs {
  return { pinnedPos: {}, shelved: [], pinnedIn: [], groups: {}, groupNames: {} };
}

export interface WorkingSetGraph {
  nodes: WorkingNode[];
  edges: WorkingEdge[];
  clusters: WorkingCluster[];
  boxes: Map<string, NodeBox>;
  /** Node IDs needing human action, ordered most urgent first (for Tab hopping). */
  actionOrder: string[];
  folded: {
    quietSessions: Placed[];
    shelvedSessions: Placed[];
    collapsedRuns: number;
    foldedHelpers: number;
  };
}

/** Optional board scope when the Map renders a saved Board (`boards/model.ts`), so every Board IS
 *  a Working Set: membership comes from the board's rule, `added` and `removed`. */
export interface BoardScope {
  board: BoardDoc;
  /** Whether a card (`refId`) belongs to this board (`boards/rules.ts` `memberTest`). */
  member: (card: string) => boolean;
  /** Cards the board shows (`boardLists.ts` `canvasCards`), including sessions reached via a matched todo/worktree/PR. */
  shows: ReadonlySet<string>;
}

export interface WorkingSetInput {
  entities: Entities;
  schedules: ScheduleItem[];
  prs: PrItem[];
  todos: Todo[];
  labels: Map<string, Label>;
  now: number;
  prefs: WorkingSetPrefs;
  /** When set, scopes membership, shelving and pull-in to this board's rule, added and removed lists. */
  boardScope?: BoardScope;
}

/* ---- Action tier, deadlines & crisp summaries ---- */

const DEADLINE_RE =
  /\b(by|due|before)\s+((?:(?:Mon|Tue|Wed|Thu|Fri|Sat|Sun)(?:day)?\.?\s+)?(?:(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2}|\d{1,2}[-/]\d{1,2}|today|tomorrow|tonight|Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday|\d{1,2}:\d{2}(?:\s*[AP]M)?))/i;

/** Pulls a short human deadline phrase out of a session's note (e.g. "by Fri Dec 4"). */
export function extractDeadline(text: string | null | undefined): string | undefined {
  if (!text) return undefined;
  const m = DEADLINE_RE.exec(text);
  if (!m) return undefined;
  return `${m[1].toLowerCase()} ${m[2].replace(/\.$/, '').trim()}`;
}

/** Formats a timestamp as a compact upcoming timer string ("19:00", "Sat 00:55", "in 45m"). */
export function shortTimer(at: number, now: number): string {
  const diff = at - now;
  if (diff <= 0) return 'due now';
  if (diff < 3_600_000) return `in ${Math.max(1, Math.round(diff / 60_000))}m`;
  const d = new Date(at);
  const today = new Date(now);
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  if (d.getFullYear() === today.getFullYear() && d.getMonth() === today.getMonth() && d.getDate() === today.getDate()) {
    return `${hh}:${mm}`;
  }
  const day = d.toLocaleDateString('en-US', { weekday: 'short' });
  return diff < 6 * DAY_MS ? `${day} ${hh}:${mm}` : `${d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })} ${hh}:${mm}`;
}

/** Classifies a session into one of five unmistakable action tiers. */
export function actionOf(s: Session, prs: Pr[], timers: SessionTimer[], now: number): ActionBadge {
  const ins = s.insight;
  const deadline = extractDeadline(ins?.note) ?? extractDeadline(s.excerpt);
  const ageMs = now - s.lastActivity;
  const stoppedOld = s.status === 'stopped' && ageMs > 2 * DAY_MS;

  if (s.status === 'needs') {
    const label = s.need === 'question' || s.asks ? 'Asks you' : s.need === 'approval' ? 'Approve' : s.need === 'message' ? 'Message' : s.need === 'stuck' ? 'Stuck' : 'Approve';
    return { tier: 'urgent', label, deadline, rank: 0 };
  }
  if (s.status === 'error' || s.limit || s.signedOut || s.apiError || s.cutOff) {
    const label = s.limit ? 'Limit hit' : s.signedOut ? 'Signed out' : s.cutOff ? 'Cut off' : 'Error';
    return { tier: 'urgent', label, deadline, rank: 0 };
  }
  if (ins?.ending === 'blocked' && !s.finished && !stoppedOld) {
    return { tier: 'urgent', label: 'Blocked', deadline, rank: 0 };
  }

  const openPrs = prs.filter((p) => p.state !== 'merged' && p.state !== 'closed');
  const failingPr = openPrs.find((p) => p.checks.failed > 0);
  const readyPr = openPrs.find((p) => prReady(p));

  const looksDone = !!s.finished || (ins?.finished ?? 0) >= 0.75;
  const activeOrLive = s.status === 'unread' || s.status === 'working' || s.live;
  if (ins?.ending === 'handoff' && !looksDone && (activeOrLive || (!!deadline && ageMs <= 3 * DAY_MS))) {
    return { tier: 'handoff', label: 'Your step', deadline, rank: 1 };
  }
  if (failingPr && (activeOrLive || ageMs <= 3 * DAY_MS)) {
    return { tier: 'handoff', label: `PR #${failingPr.number} failing`, deadline, rank: 1 };
  }
  if (readyPr && (activeOrLive || ageMs <= 3 * DAY_MS)) {
    return { tier: 'handoff', label: `Merge #${readyPr.number}`, deadline, rank: 1 };
  }

  if (!looksDone && !stoppedOld && (ins?.ending === 'question' || ins?.ending === 'offer') && (s.status === 'unread' || s.live)) {
    return { tier: 'decision', label: ins.ending === 'question' ? 'Question' : 'Decision', deadline, rank: 2 };
  }
  if (s.status === 'unread' && !looksDone && ins?.ending !== 'report') {
    return { tier: 'decision', label: 'Your turn', deadline, rank: 2 };
  }

  if (s.status === 'working') {
    const label = ins?.background === 'polling' ? 'Polling' : 'Working';
    return { tier: 'working', label, rank: 3 };
  }
  if (timers.length > 0 || (ins?.waitsUntil && ins.waitsUntil > now && ageMs <= 3 * DAY_MS)) {
    const nextAt = timers[0]?.at ?? ins?.waitsUntil;
    return { tier: 'working', label: 'Scheduled', deadline: nextAt ? shortTimer(nextAt, now) : undefined, rank: 3 };
  }

  if (s.status === 'unread' && ins?.ending === 'report') {
    return { tier: 'quiet', label: 'FYI', rank: 4 };
  }
  return { tier: 'quiet', label: looksDone ? 'Done' : s.status === 'stopped' ? 'Stopped' : 'Idle', rank: 4 };
}

/** Strips timing tokens and trims raw noise into a crisp 1–2 line summary of what a session owns. */
export function summaryOf(s: Session, now: number): string {
  const note = s.insight?.note?.trim();
  if (note) return note;
  if (s.limit) {
    const resets = s.limit.resetsAt ? ` · resets ${resetsPhrase(s.limit.resetsAt, now)}` : '';
    return `${s.account} hit its usage limit${resets}`;
  }
  if (s.signedOut) return `${s.account} sign-in expired — sign in to continue`;
  if (s.apiError?.said) return s.apiError.said;
  const raw = sessionLine(s)
    .replace(/\{for:(-?\d+)\}/g, (_, ts) => duration(Number(ts), now))
    .replace(/\{ago:(-?\d+)\}/g, '')
    .replace(/\s+/g, ' ')
    .trim();
  return raw || s.headline;
}

/* ---- Schedule classification ---- */

function isCheckpointSchedule(sc: ScheduleItem): boolean {
  return runsOnce(sc) || sc.run.to === 'session';
}

function isRecurringPipeline(sc: ScheduleItem): boolean {
  return !isCheckpointSchedule(sc) && !sc.orchestrates && !sc.paused && sc.finished == null && sc.finishedAt == null;
}

function scheduleHubId(id: string): string {
  return `schedule:${id}`;
}

/* ---- Text tokenization & TF-IDF similarity (generic, zero project-specific words) ---- */

const STOP_WORDS = new Set([
  'the', 'and', 'for', 'with', 'from', 'this', 'that', 'your', 'you', 'are', 'was', 'were', 'has', 'have', 'had',
  'not', 'but', 'all', 'any', 'can', 'will', 'into', 'out', 'off', 'over', 'under', 'only', 'just', 'now', 'yet',
  'still', 'more', 'most', 'some', 'each', 'both', 'than', 'then', 'when', 'what', 'where', 'which', 'who', 'why',
  'how', 'also', 'too', 'very', 'been', 'being', 'does', 'did', 'done', 'doing', 'make', 'made', 'take', 'took',
  'give', 'gave', 'sent', 'send', 'keep', 'kept', 'left', 'open', 'closed', 'close', 'new', 'old', 'next', 'last',
  'first', 'other', 'same', 'once', 'again', 'after', 'before', 'until', 'since', 'about', 'there', 'here', 'their',
  'them', 'they', 'its', 'our', 'his', 'her', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight',
  // Generic coding-agent workflow vocabulary (not domain topics)
  'session', 'sessions', 'helper', 'helpers', 'worktree', 'worktrees', 'branch', 'branches', 'repo', 'repos',
  'main', 'master', 'origin', 'merge', 'merged', 'merging', 'commit', 'commits', 'push', 'pushed', 'pull', 'request',
  'review', 'reviewed', 'reviewer', 'approval', 'approved', 'approve', 'check', 'checks', 'checked', 'checking',
  'pass', 'passed', 'passing', 'fail', 'failed', 'failing', 'status', 'update', 'updated', 'fix', 'fixed', 'fixes',
  'run', 'runs', 'running', 'started', 'start', 'stop', 'stopped', 'idle', 'working', 'waiting', 'waits', 'wait',
  'asks', 'ask', 'asked', 'needs', 'need', 'needed', 'turn', 'step', 'steps', 'handoff', 'report', 'draft', 'drafted',
  'ready', 'blocked', 'nothing', 'pending', 'left', 'note', 'notes', 'todo', 'todos', 'fyi', 'pure', 'confirm',
  'confirmed', 'confirming', 'file', 'files', 'folder', 'path', 'code', 'line', 'lines', 'test', 'tests', 'build',
  'today', 'tomorrow', 'yesterday', 'week', 'weekly', 'daily', 'morning', 'night', 'nightly', 'month', 'monthly',
  'mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun', 'monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday', 'sunday',
  'jan', 'feb', 'mar', 'apr', 'may', 'jun', 'jul', 'aug', 'sep', 'oct', 'nov', 'dec',
  'utc', 'pst', 'pdt', 'est', 'edt', 'gmt', 'claude', 'codex', 'cursor', 'antigravity', 'opencode', 'codealong',
]);

const HEX_8_RE = /\b[0-9a-f]{8}\b/gi;
const PR_REF_RE = /(?:([a-z0-9._-]+)#|#)(\d{2,5})\b/gi;

interface TermVec {
  weights: Map<string, number>;
  display: Map<string, string>;
}

/** Extracts weighted domain terms from a session's title, branch, labels, repo and note. */
function tokenize(text: string, boost: number, out: Map<string, number>, display: Map<string, string>) {
  if (!text) return;
  // Keep meaningful hyphenated compounds (e.g. "phase-b", "ops-2026-042", "req-014") and words.
  const rawTokens = text.match(/[A-Za-z][A-Za-z0-9_-]{1,32}|[A-Za-z0-9]+-[A-Za-z0-9-]+/g) ?? [];
  for (const raw of rawTokens) {
    const clean = raw.replace(/^[_-]+|[_-]+$/g, '');
    if (clean.length < 3) continue;
    // Skip pure hex hashes (handled separately as session ID mentions).
    if (/^[0-9a-f]{7,}$/i.test(clean)) continue;
    const low = clean.toLowerCase();
    if (!STOP_WORDS.has(low) && !/^\d+$/.test(low)) {
      out.set(low, (out.get(low) ?? 0) + boost);
      if (!display.has(low) || /[A-Z]/.test(clean)) display.set(low, clean);
    }
    // Also index sub-parts of hyphenated/underscored identifiers so "kestrel-release" meets "Kestrel".
    if (clean.includes('-') || clean.includes('_')) {
      const parts = clean.split(/[-_]+/);
      for (const p of parts) {
        const pl = p.toLowerCase();
        if (pl.length >= 3 && !STOP_WORDS.has(pl) && !/^\d+$/.test(pl) && !/^[0-9a-f]{7,}$/i.test(pl)) {
          out.set(pl, (out.get(pl) ?? 0) + boost * 0.65);
          if (!display.has(pl) || /[A-Z]/.test(p)) display.set(pl, p);
        }
      }
    }
  }
}

function sessionTerms(s: Session, lane: Lane, repoBoost: number): TermVec {
  const weights = new Map<string, number>();
  const display = new Map<string, string>();
  tokenize(s.title, 2.4, weights, display);
  for (const l of s.labels ?? []) tokenize(l.name, 2.6, weights, display);
  if (s.branch && s.branch !== 'main' && s.branch !== 'master') tokenize(s.branch, 1.4, weights, display);
  if (s.insight?.note) tokenize(s.insight.note, 1.15, weights, display);
  else if (s.excerpt) tokenize(s.excerpt.slice(0, 180), 0.7, weights, display);
  if (lane.repo && repoBoost > 0) tokenize(lane.repo, repoBoost, weights, display);
  return { weights, display };
}

function scheduleTerms(sc: ScheduleItem): TermVec {
  const weights = new Map<string, number>();
  const display = new Map<string, string>();
  tokenize(sc.name, 2.5, weights, display);
  tokenize(sc.run.prompt.slice(0, 200), 0.6, weights, display);
  return { weights, display };
}

/** Extracts PR references as normalized keys (`repo#num` when prefixed, plus `#num`). */
function prRefsOf(s: Session, lane: Lane, prs: Pr[]): Set<string> {
  const out = new Set<string>();
  const repo = lane.repo.toLowerCase();
  for (const p of prs) {
    out.add(`#${p.number}`);
    if (repo) out.add(`${repo}#${p.number}`);
  }
  const text = `${s.title} ${s.insight?.note ?? ''} ${s.excerpt ?? ''}`;
  for (const m of text.matchAll(PR_REF_RE)) {
    const num = m[2];
    const pref = m[1]?.toLowerCase();
    out.add(`#${num}`);
    if (pref) out.add(`${pref}#${num}`);
  }
  return out;
}

/* ---- Cluster colors ---- */

const CLUSTER_COLORS: LabelColor[] = ['blue', 'teal', 'purple', 'indigo', 'green', 'pink', 'red'];

function titleCase(word: string): string {
  if (/[A-Z]{2,}/.test(word)) return word;
  return word.charAt(0).toUpperCase() + word.slice(1);
}

/* ---- Main builder ---- */

export function buildWorkingSet(input: WorkingSetInput): WorkingSetGraph {
  const { entities, schedules, prs, todos, now, prefs, boardScope } = input;
  const board = boardScope?.board ?? null;
  const isEverything = !board || takesAll(board.rule);
  const isHandBoard = !!board && board.rule === null;
  const isRuleBoard = !!board && board.rule !== null && !takesAll(board.rule);
  const ruleHasLiveness = !!(board?.rule && (board.rule.statuses?.length || board.rule.stages?.length || board.rule.active));

  const shelvedSet = new Set([...(board?.removed ?? []), ...prefs.shelved]);
  const pinnedInSet = new Set([...(board?.added ?? []), ...prefs.pinnedIn]);
  const viaAddedTodo = new Set<string>();
  const viaAddedPr = new Set<string>();
  if (board) {
    for (const t of todos) {
      if (pinnedInSet.has(refId('todo', t.id))) for (const k of t.sessions) viaAddedTodo.add(k);
    }
    for (const p of prs) {
      if (pinnedInSet.has(refId('pr', p.key))) for (const ps of p.sessions) viaAddedPr.add(ps.key);
    }
  }
  const inScopedBoard = (sKey: string, laneId: string): boolean => {
    if (isEverything) return true;
    const id = refId('session', sKey);
    if (pinnedInSet.has(id) || pinnedInSet.has(refId('worktree', laneId)) || viaAddedTodo.has(sKey) || viaAddedPr.has(sKey)) return true;
    if (isHandBoard) return !!boardScope?.shows.has(id);
    return !!(boardScope?.member(id) || boardScope?.shows.has(id));
  };

  // 1. Classify schedules into session checkpoints vs recurring monitor hubs.
  const timersBySession = new Map<string, SessionTimer[]>();
  const recurringBySession = new Map<string, ScheduleItem>();
  const collapsedBySchedule = new Map<string, number>();
  const scopedCollapsedBySchedule = new Map<string, number>();
  const collapsedSessionKeys = new Set<string>();
  const hubCandidates = new Map<string, ScheduleItem>();

  const projectLaneIds = new Set(entities.lanes.keys());
  const projectSessionKeys = new Set(entities.sessions.keys());

  for (const sc of schedules) {
    if (sc.orchestrates) continue;
    const targetKey = sc.thread ?? sc.run.session ?? '';
    const runKeys = (sc.runs ?? []).map((r) => r.session).filter((k): k is string => !!k);
    const touchesProject =
      (targetKey && projectSessionKeys.has(targetKey)) ||
      runKeys.some((k) => projectSessionKeys.has(k)) ||
      (sc.worktree ? projectLaneIds.has(sc.worktree) : false);
    if (!touchesProject) continue;

    if (isCheckpointSchedule(sc)) {
      if (sc.paused || sc.finished != null || sc.finishedAt != null) continue;
      const at = sc.nextAt ?? sc.queued?.at;
      if (!at || !targetKey || !projectSessionKeys.has(targetKey)) continue;
      const list = timersBySession.get(targetKey) ?? [];
      list.push({
        scheduleId: sc.id,
        name: sc.name,
        at,
        queued: !!sc.queued,
        words: sc.queued ? 'queued' : shortTimer(at, now),
      });
      timersBySession.set(targetKey, list);
      continue;
    }

    if (isRecurringPipeline(sc)) {
      hubCandidates.set(sc.id, sc);
      const latestRunKey = sc.thread ?? runKeys[0];
      const seen = new Set<string>();
      for (const k of [sc.thread, ...runKeys].filter((x): x is string => !!x)) {
        if (seen.has(k)) continue;
        seen.add(k);
        if (!projectSessionKeys.has(k)) continue;
        recurringBySession.set(k, sc);
        // Collapse superseded `to=new` runs that are idle/stopped into the schedule hub.
        if (sc.run.to === 'new' && k !== latestRunKey) {
          const placed = entities.sessions.get(k);
          if (placed && (placed.session.status === 'idle' || placed.session.status === 'stopped') && !pinnedInSet.has(refId('session', k))) {
            collapsedSessionKeys.add(k);
            collapsedBySchedule.set(sc.id, (collapsedBySchedule.get(sc.id) ?? 0) + 1);
            if (inScopedBoard(k, placed.lane.id)) {
              scopedCollapsedBySchedule.set(sc.id, (scopedCollapsedBySchedule.get(sc.id) ?? 0) + 1);
            }
          }
        }
      }
    }
  }

  for (const list of timersBySession.values()) list.sort((a, b) => a.at - b.at);

  // Also fold superseded title-numbered schedule runs ("<Name>: <digits>") when schedules aren't attached.
  const RUN_PREFIX_RE = /^(.{4,}?):\s*\d{5,}$/;
  const latestByPrefix = new Map<string, { key: string; at: number }>();
  for (const placed of entities.sessions.values()) {
    const s = placed.session;
    if (s.archived) continue;
    const m = RUN_PREFIX_RE.exec(s.title);
    if (!m) continue;
    const pref = m[1].trim().toLowerCase();
    const cur = latestByPrefix.get(pref);
    if (!cur || s.lastActivity > cur.at) latestByPrefix.set(pref, { key: s.key, at: s.lastActivity });
  }
  let extraCollapsedRuns = 0;
  for (const placed of entities.sessions.values()) {
    const s = placed.session;
    if (s.archived || collapsedSessionKeys.has(s.key) || pinnedInSet.has(refId('session', s.key))) continue;
    const m = RUN_PREFIX_RE.exec(s.title);
    if (!m) continue;
    const best = latestByPrefix.get(m[1].trim().toLowerCase());
    if (best && best.key !== s.key && (s.status === 'idle' || s.status === 'stopped')) {
      collapsedSessionKeys.add(s.key);
      if (inScopedBoard(s.key, placed.lane.id)) extraCollapsedRuns += 1;
    }
  }

  // 2. Fold idle/review helpers and admit working-set sessions (scoped to the board when provided).
  const helpersByParent = new Map<string, Placed[]>();
  const quietSessions: Placed[] = [];
  const quietMatchesRule = new Set<string>();
  const shelvedSessions: Placed[] = [];
  const sessionNodes = new Map<string, SessionNode>();
  let allFoldedHelpers = 0;

  for (const placed of entities.sessions.values()) {
    const { session: s, lane } = placed;
    const id = refId('session', s.key);
    const explicitlyAdded = pinnedInSet.has(id);
    const viaAddedParent = !!board && (pinnedInSet.has(refId('worktree', lane.id)) || viaAddedTodo.has(s.key) || viaAddedPr.has(s.key));

    if (s.archived && !explicitlyAdded) continue;
    if (s.orchestrator && !explicitlyAdded) continue;
    if (s.headless && !explicitlyAdded && s.status !== 'needs' && s.status !== 'error') continue;
    if (collapsedSessionKeys.has(s.key)) continue;

    const sPrs = sessionPrs(prs, lane, s);
    const timers = timersBySession.get(s.key) ?? [];
    const action = actionOf(s, sPrs, timers, now);

    if (shelvedSet.has(id) || shelvedSet.has(s.key)) {
      shelvedSessions.push(placed);
      continue;
    }

    // Helpers: surface immediately if urgent or actively working (or explicitly added); else fold onto parent.
    if (s.helper && !explicitlyAdded) {
      const activeHelper = action.tier === 'urgent' || (s.status === 'working' && !s.helper.pr);
      if (!activeHelper) {
        allFoldedHelpers += 1;
        if (s.helper.parent) {
          const list = helpersByParent.get(s.helper.parent) ?? [];
          list.push(placed);
          helpersByParent.set(s.helper.parent, list);
        }
        continue;
      }
    }

    const recent3d = now - s.lastActivity <= 3 * DAY_MS;
    const activeStatus = s.status === 'needs' || s.status === 'error' || s.status === 'working' || s.status === 'unread';
    const autoAdmit =
      activeStatus ||
      timers.length > 0 ||
      action.tier === 'urgent' ||
      action.tier === 'decision' ||
      action.tier === 'working' ||
      (action.tier === 'handoff' && recent3d);

    let admit = false;
    if (explicitlyAdded || viaAddedParent) {
      admit = true;
    } else if (isEverything) {
      admit = autoAdmit;
    } else if (isHandBoard) {
      admit = !!boardScope?.shows.has(id);
    } else if (isRuleBoard) {
      const matched = !!(boardScope?.member(id) || boardScope?.shows.has(id));
      if (matched) {
        const looksDone = !!s.finished || (s.insight?.finished ?? 0) >= 0.75;
        admit = ruleHasLiveness || autoAdmit || (!looksDone && now - s.lastActivity <= 7 * DAY_MS);
        if (!admit) quietMatchesRule.add(s.key);
      }
    }

    if (!admit) {
      quietSessions.push(placed);
      continue;
    }

    sessionNodes.set(id, {
      kind: 'session',
      id,
      key: s.key,
      session: s,
      lane,
      action,
      summary: summaryOf(s, now),
      timers,
      helpers: [],
      prs: sPrs,
      scheduleName: recurringBySession.get(s.key)?.name,
      pinned: !!prefs.pinnedPos[id],
    });
  }

  let scopedFoldedHelpers = 0;
  for (const node of sessionNodes.values()) {
    node.helpers = (helpersByParent.get(node.key) ?? []).sort((a, b) => b.session.lastActivity - a.session.lastActivity);
    scopedFoldedHelpers += node.helpers.length;
  }
  const foldedHelpers = isEverything ? allFoldedHelpers : scopedFoldedHelpers;
  quietSessions.sort(
    (a, b) =>
      Number(quietMatchesRule.has(b.session.key)) - Number(quietMatchesRule.has(a.session.key)) ||
      b.session.lastActivity - a.session.lastActivity,
  );
  shelvedSessions.sort((a, b) => b.session.lastActivity - a.session.lastActivity);

  // 3. Admit relevant Schedule Hubs (scoped to this board when focused).
  const hubNodes = new Map<string, ScheduleHubNode>();
  const hubLinks: { hubId: string; sessionNodeId: string }[] = [];

  for (const sc of hubCandidates.values()) {
    const id = scheduleHubId(sc.id);
    if (shelvedSet.has(id)) continue;
    const quietRuns = (isEverything ? collapsedBySchedule.get(sc.id) : scopedCollapsedBySchedule.get(sc.id)) ?? 0;
    const admittedChildren: string[] = [];
    for (const node of sessionNodes.values()) {
      if (recurringBySession.get(node.key)?.id === sc.id) admittedChildren.push(node.id);
    }
    if (!isEverything && admittedChildren.length === 0 && quietRuns === 0) continue;
    // Only surface a standalone hub when it has collapsed runs to represent OR multiple runs OR a problem;
    // a single-thread schedule whose one session is already on the desk just badges that session directly.
    const needsHub = quietRuns > 0 || admittedChildren.length > 1 || !!sc.problem;
    if (!needsHub) continue;
    // Skip silent housekeeping schedules with zero admitted sessions and zero problems.
    if (sc.quiet && admittedChildren.length === 0 && !sc.problem) continue;

    const nextLabel = sc.problem
      ? sc.problem
      : sc.nextAt
        ? `Next ${shortTimer(sc.nextAt, now)}`
        : sc.whenWords;
    hubNodes.set(id, {
      kind: 'schedule',
      id,
      schedule: sc,
      quietRuns,
      totalRuns: (sc.runs ?? []).length,
      nextLabel,
      latestRunKey: sc.thread ?? sc.runs?.[0]?.session,
      action: sc.problem ? { tier: 'urgent', label: 'Can’t run', rank: 0 } : { tier: 'quiet', label: 'Schedule', rank: 4 },
      pinned: !!prefs.pinnedPos[id],
    });
    for (const childId of admittedChildren) hubLinks.push({ hubId: id, sessionNodeId: childId });
  }

  const nodes: WorkingNode[] = [...sessionNodes.values(), ...hubNodes.values()];
  const nodeById = new Map(nodes.map((n) => [n.id, n]));

  // 4. Build weighted relatedness edges across all admitted nodes.
  const edges: WorkingEdge[] = [];
  const edgeSeen = new Set<string>();
  const addEdge = (from: string, to: string, kind: WorkingEdgeKind, weight: number, label?: string) => {
    if (from === to || !nodeById.has(from) || !nodeById.has(to)) return;
    const pair = from < to ? `${from}|${to}` : `${to}|${from}`;
    if (edgeSeen.has(pair)) return;
    edgeSeen.add(pair);
    edges.push({ id: `${kind}:${pair}`, from, to, kind, weight, label });
  };

  // 4a. Schedule hub links
  for (const l of hubLinks) addEdge(l.hubId, l.sessionNodeId, 'schedule', 0.92, 'runs');

  // 4b. Helper, origin, peer links
  const shortIdToNode = new Map<string, string>();
  const prToNodes = new Map<string, string[]>();
  const todoToNodes = new Map<string, string[]>();
  const laneToNodes = new Map<string, string[]>();

  for (const n of sessionNodes.values()) {
    const s = n.session;
    if (s.id.length >= 8) shortIdToNode.set(s.id.slice(0, 8).toLowerCase(), n.id);
    if (s.helper?.parent) addEdge(refId('session', s.helper.parent), n.id, 'helper', 1.0, 'helper');
    if (s.origin) addEdge(refId('session', s.origin.key), n.id, 'origin', 1.0, s.origin.how);
    for (const peer of s.peers ?? []) addEdge(n.id, refId('session', peer.key), 'peer', 0.94, 'messages');

    for (const prRef of prRefsOf(s, n.lane, n.prs)) {
      const list = prToNodes.get(prRef) ?? [];
      list.push(n.id);
      prToNodes.set(prRef, list);
    }
    const isDedicatedWorktree = n.lane.origin !== 'main' && n.lane.branch && n.lane.branch !== 'main' && n.lane.branch !== 'master';
    if (isDedicatedWorktree) {
      const list = laneToNodes.get(n.lane.id) ?? [];
      list.push(n.id);
      laneToNodes.set(n.lane.id, list);
    }
  }

  for (const t of todos) {
    if (t.status === 'done' || t.archivedAt) continue;
    const linked = t.sessions.map((k) => refId('session', k)).filter((id) => sessionNodes.has(id));
    if (linked.length >= 2) todoToNodes.set(t.id, linked);
  }

  // 4c. Cross-referenced short session IDs in insight.note / excerpt (catches links across repos!)
  for (const n of sessionNodes.values()) {
    const hay = `${n.session.insight?.note ?? ''} ${n.session.excerpt ?? ''}`;
    for (const m of hay.matchAll(HEX_8_RE)) {
      const target = shortIdToNode.get(m[0].toLowerCase());
      if (target && target !== n.id) addEdge(n.id, target, 'mention', 0.9, 'refs');
    }
  }
  // Also connect schedule hubs whose collapsed runs' notes referenced an admitted remediation session!
  for (const hub of hubNodes.values()) {
    for (const r of hub.schedule.runs ?? []) {
      if (!r.session) continue;
      const runPlaced = entities.sessions.get(r.session);
      if (!runPlaced) continue;
      const hay = `${runPlaced.session.insight?.note ?? ''} ${runPlaced.session.excerpt ?? ''}`;
      for (const m of hay.matchAll(HEX_8_RE)) {
        const target = shortIdToNode.get(m[0].toLowerCase());
        if (target) addEdge(hub.id, target, 'schedule', 0.88, 'spawned');
      }
    }
  }

  // 4d. Shared PR references
  for (const [prRef, list] of prToNodes) {
    if (!prRef.includes('#') || list.length < 2 || list.length > 5) continue;
    // Prefer repo-qualified keys when present; skip bare #N if we already have repo#N edges
    for (let i = 0; i < list.length; i++) {
      for (let j = i + 1; j < list.length; j++) {
        addEdge(list[i], list[j], 'pr', 0.86, prRef);
      }
    }
  }

  // 4e. Shared open todo
  for (const list of todoToNodes.values()) {
    for (let i = 0; i < list.length; i++) {
      for (let j = i + 1; j < list.length; j++) {
        addEdge(list[i], list[j], 'mention', 0.82, 'same issue');
      }
    }
  }

  // 4f. Shared dedicated worktree
  for (const list of laneToNodes.values()) {
    for (let i = 0; i < list.length; i++) {
      for (let j = i + 1; j < list.length; j++) {
        addEdge(list[i], list[j], 'worktree', 0.65, 'same worktree');
      }
    }
  }

  // 4g. Lightweight TF-IDF semantic topic affinity (groups related sessions across repos/worktrees!)
  const repoNodes = new Map<string, string[]>();
  const repoStopTerms = new Set<string>();
  for (const n of sessionNodes.values()) {
    const r = n.lane.repo.toLowerCase();
    if (!r) continue;
    const list = repoNodes.get(r) ?? [];
    list.push(n.id);
    repoNodes.set(r, list);
    repoStopTerms.add(r);
    for (const part of r.split(/[-_]+/)) if (part.length >= 3) repoStopTerms.add(part);
  }
  const sessionTotal = Math.max(1, sessionNodes.size);
  // Sessions in a dedicated secondary repo (< 40% of the working set, 2..5 sessions) share a clear domain boundary.
  for (const [repo, ids] of repoNodes) {
    if (ids.length < 2 || ids.length > 5 || ids.length >= sessionTotal * 0.4) continue;
    for (let i = 0; i < ids.length - 1; i++) addEdge(ids[i], ids[i + 1], 'topic', 0.56, repo);
  }

  const vecs = new Map<string, TermVec>();
  const df = new Map<string, number>();
  for (const n of nodes) {
    let v: TermVec;
    if (n.kind === 'session') {
      const rCount = repoNodes.get(n.lane.repo.toLowerCase())?.length ?? 0;
      const repoBoost = rCount > 0 && rCount < sessionTotal * 0.45 ? 1.25 : 0;
      v = sessionTerms(n.session, n.lane, repoBoost);
    } else {
      v = scheduleTerms(n.schedule);
    }
    vecs.set(n.id, v);
    for (const term of v.weights.keys()) df.set(term, (df.get(term) ?? 0) + 1);
  }
  const N = Math.max(1, nodes.length);
  const idf = (term: string) => Math.log((N + 1) / ((df.get(term) ?? 1) + 1)) + 1;

  const norms = new Map<string, number>();
  for (const [id, v] of vecs) {
    let sum = 0;
    for (const [t, w] of v.weights) {
      const tw = w * idf(t);
      sum += tw * tw;
    }
    norms.set(id, Math.sqrt(sum) || 1);
  }

  // Keep up to top 2 topic neighbors per node so neighborhoods stay coherent, not hairballs.
  const topicPairs: { a: string; b: string; sim: number; topTerm: string }[] = [];
  for (let i = 0; i < nodes.length; i++) {
    for (let j = i + 1; j < nodes.length; j++) {
      const a = nodes[i];
      const b = nodes[j];
      const va = vecs.get(a.id)!;
      const vb = vecs.get(b.id)!;
      let dot = 0;
      let bestTerm = '';
      let bestContrib = 0;
      for (const [t, wa] of va.weights) {
        const wb = vb.weights.get(t);
        if (!wb) continue;
        if ((df.get(t) ?? 0) > Math.max(4, Math.ceil(N * 0.55))) continue;
        const w = idf(t);
        const contrib = wa * w * (wb * w);
        dot += contrib;
        if (contrib > bestContrib) {
          bestContrib = contrib;
          bestTerm = va.display.get(t) ?? vb.display.get(t) ?? t;
        }
      }
      const sim = dot / (norms.get(a.id)! * norms.get(b.id)!);
      if (sim >= 0.13 && bestTerm) topicPairs.push({ a: a.id, b: b.id, sim, topTerm: bestTerm });
    }
  }
  topicPairs.sort((x, y) => y.sim - x.sim);
  const topicDeg = new Map<string, number>();
  for (const p of topicPairs) {
    if ((topicDeg.get(p.a) ?? 0) >= 2 || (topicDeg.get(p.b) ?? 0) >= 2) continue;
    const weight = Math.min(0.78, 0.48 + p.sim * 0.65);
    addEdge(p.a, p.b, 'topic', weight, p.topTerm);
    topicDeg.set(p.a, (topicDeg.get(p.a) ?? 0) + 1);
    topicDeg.set(p.b, (topicDeg.get(p.b) ?? 0) + 1);
  }

  // 5. Form clusters via union-find (honoring user manual groups, avoiding mega-blobs).
  const parent = new Map<string, string>(nodes.map((n) => [n.id, n.id]));
  const find = (x: string): string => {
    let r = x;
    while (parent.get(r) !== r) r = parent.get(r)!;
    while (parent.get(x) !== r) {
      const next = parent.get(x)!;
      parent.set(x, r);
      x = next;
    }
    return r;
  };
  const union = (a: string, b: string) => {
    const ra = find(a);
    const rb = find(b);
    if (ra !== rb) parent.set(rb, ra);
  };

  // Manual user groups take absolute precedence.
  const manualMembers = new Map<string, string[]>();
  const manuallyPlaced = new Set<string>();
  for (const [nid, gid] of Object.entries(prefs.groups)) {
    if (!nodeById.has(nid)) continue;
    manuallyPlaced.add(nid);
    const list = manualMembers.get(gid) ?? [];
    list.push(nid);
    manualMembers.set(gid, list);
  }
  for (const members of manualMembers.values()) {
    for (let i = 1; i < members.length; i++) union(members[0], members[i]);
  }

  // Degree count of mention edges to avoid letting a daily digest session collapse unrelated workstreams.
  const mentionDeg = new Map<string, number>();
  for (const e of edges) {
    if (e.kind === 'mention') {
      mentionDeg.set(e.from, (mentionDeg.get(e.from) ?? 0) + 1);
      mentionDeg.set(e.to, (mentionDeg.get(e.to) ?? 0) + 1);
    }
  }

  const sortedEdges = [...edges].sort((a, b) => b.weight - a.weight);
  const componentSize = (id: string) => {
    const r = find(id);
    let c = 0;
    for (const n of nodes) if (find(n.id) === r) c++;
    return c;
  };
  for (const e of sortedEdges) {
    if (e.weight < 0.5) continue;
    // Don't auto-merge nodes the user explicitly grouped or detached elsewhere.
    if (manuallyPlaced.has(e.from) || manuallyPlaced.has(e.to)) continue;
    // A broad digest session citing 3+ unrelated threads stays a bridge instead of merging everything.
    if (e.kind === 'mention' && ((mentionDeg.get(e.from) ?? 0) >= 3 || (mentionDeg.get(e.to) ?? 0) >= 3)) continue;
    if (find(e.from) === find(e.to)) continue;
    // Cap auto-cluster size at 7 so neighborhoods stay readable at a glance.
    if (componentSize(e.from) + componentSize(e.to) > 7) continue;
    union(e.from, e.to);
  }

  const rawGroups = new Map<string, string[]>();
  for (const n of nodes) {
    const r = find(n.id);
    const list = rawGroups.get(r) ?? [];
    list.push(n.id);
    rawGroups.set(r, list);
  }

  // Mark cross-cluster edges for softer visual rendering.
  const clusterOfNode = new Map<string, string>();
  const clusters: WorkingCluster[] = [];

  for (const memberIds of rawGroups.values()) {
    if (memberIds.length < 2 && !memberIds.some((id) => prefs.groups[id])) continue;
    const manualGid = memberIds.map((id) => prefs.groups[id]).find(Boolean);
    const stableKey = manualGid ?? `cl:${[...memberIds].sort()[0]}`;
    for (const id of memberIds) clusterOfNode.set(id, stableKey);

    const customName = prefs.groupNames[stableKey];
    const title = customName ?? inferClusterTitle(memberIds, nodeById, vecs, df, N, repoStopTerms);
    const color = inferClusterColor(memberIds, nodeById, title);

    let urgentCount = 0;
    let actionCount = 0;
    let workingCount = 0;
    for (const id of memberIds) {
      const n = nodeById.get(id)!;
      if (n.action.tier === 'urgent') urgentCount++;
      if (n.action.rank <= 2) actionCount++;
      if (n.action.tier === 'working') workingCount++;
    }

    clusters.push({
      id: stableKey,
      title,
      color,
      nodeIds: memberIds.sort((a, b) => nodeById.get(a)!.action.rank - nodeById.get(b)!.action.rank || a.localeCompare(b)),
      x: 0,
      y: 0,
      w: 0,
      h: 0,
      urgentCount,
      actionCount,
      workingCount,
      custom: !!customName || !!manualGid,
    });
  }

  for (const e of edges) {
    const ca = clusterOfNode.get(e.from);
    const cb = clusterOfNode.get(e.to);
    e.crossCluster = !ca || !cb || ca !== cb;
  }

  // 6. Deterministic damped force-directed layout + overlap-free rectangular packing.
  const boxes = layoutWorkingSet(nodes, edges, clusters, prefs.pinnedPos);

  // 7. Action order for Tab hopping: urgent → handoff → decision, longest waiting first.
  const actionOrder = nodes
    .filter((n) => n.kind === 'session' && n.action.rank <= 2)
    .sort((a, b) => {
      if (a.action.rank !== b.action.rank) return a.action.rank - b.action.rank;
      const ta = a.kind === 'session' ? (a.session.statusSince ?? a.session.lastActivity) : 0;
      const tb = b.kind === 'session' ? (b.session.statusSince ?? b.session.lastActivity) : 0;
      return ta - tb;
    })
    .map((n) => n.id);

  const collapsedMap = isEverything ? collapsedBySchedule : scopedCollapsedBySchedule;
  return {
    nodes,
    edges,
    clusters,
    boxes,
    actionOrder,
    folded: {
      quietSessions,
      shelvedSessions,
      collapsedRuns: [...collapsedMap.values()].reduce((a, b) => a + b, 0) + extraCollapsedRuns,
      foldedHelpers,
    },
  };
}

function inferClusterTitle(
  memberIds: string[],
  nodeById: Map<string, WorkingNode>,
  vecs: Map<string, TermVec>,
  df: Map<string, number>,
  totalNodes: number,
  repoStopTerms: Set<string>,
): string {
  // 1. Shared session label wins if >=2 members share one.
  const labelCounts = new Map<string, number>();
  for (const id of memberIds) {
    const n = nodeById.get(id);
    if (n?.kind !== 'session') continue;
    for (const l of n.session.labels ?? []) labelCounts.set(l.name, (labelCounts.get(l.name) ?? 0) + 1);
  }
  const sharedLabel = [...labelCounts.entries()].sort((a, b) => b[1] - a[1])[0];
  if (sharedLabel && sharedLabel[1] >= 2) return sharedLabel[0];

  // 2. If a Schedule Hub anchors a small cluster, lead with its name.
  const hub = memberIds.map((id) => nodeById.get(id)).find((n): n is ScheduleHubNode => n?.kind === 'schedule');
  if (hub && memberIds.length <= 3) return hub.schedule.name;

  // 3. Otherwise pick the top 2 highest-IDF domain terms (preferring non-repo words first).
  const termScore = new Map<string, { score: number; count: number; display: string; isRepo: boolean }>();
  for (const id of memberIds) {
    const v = vecs.get(id);
    if (!v) continue;
    for (const [t, w] of v.weights) {
      const freq = df.get(t) ?? 1;
      const idf = Math.log((totalNodes + 1) / (freq + 1)) + 1;
      const isRepo = repoStopTerms.has(t);
      const cur = termScore.get(t) ?? { score: 0, count: 0, display: v.display.get(t) ?? t, isRepo };
      cur.score += w * idf * (isRepo ? 0.55 : 1);
      cur.count += 1;
      termScore.set(t, cur);
    }
  }
  const ranked = [...termScore.entries()]
    .filter(([t]) => t.length >= 3)
    .sort((a, b) => (b[1].count >= 2 ? 1 : 0) - (a[1].count >= 2 ? 1 : 0) || (a[1].isRepo ? 1 : 0) - (b[1].isRepo ? 1 : 0) || b[1].score - a[1].score);
  const picked: string[] = [];
  for (const [t, info] of ranked) {
    if (picked.some((p) => p.toLowerCase().includes(t) || t.includes(p.toLowerCase()))) continue;
    picked.push(titleCase(info.display.replace(/[-_]+/g, ' ')));
    if (picked.length === 2) break;
  }
  return picked.length > 0 ? picked.join(' · ') : 'Related work';
}

function inferClusterColor(memberIds: string[], nodeById: Map<string, WorkingNode>, title: string): LabelColor {
  for (const id of memberIds) {
    const n = nodeById.get(id);
    if (n?.kind === 'session' && n.session.labels?.[0]) return n.session.labels[0].color;
  }
  return CLUSTER_COLORS[hashStr(title) % CLUSTER_COLORS.length];
}

/* ---- Pure user-organization operations on WorkingSetPrefs ---- */

/** Groups `sourceId` with `targetId` (joining target's cluster, or creating a new manual cluster). */
export function groupNodes(prefs: WorkingSetPrefs, sourceId: string, targetId: string, existingClusterId?: string): WorkingSetPrefs {
  if (sourceId === targetId) return prefs;
  const gid = prefs.groups[targetId] ?? existingClusterId ?? `grp:${Date.now().toString(36)}`;
  const groups = { ...prefs.groups, [sourceId]: gid, [targetId]: gid };
  // Unpin the dropped node so it settles cleanly inside its new cluster.
  const pinnedPos = { ...prefs.pinnedPos };
  delete pinnedPos[sourceId];
  return { ...prefs, groups, pinnedPos };
}

/** Detaches `nodeId` from its current cluster into its own standalone pinned spot. */
export function detachNode(prefs: WorkingSetPrefs, nodeId: string, at: { x: number; y: number }): WorkingSetPrefs {
  const soloId = `solo:${nodeId}`;
  return {
    ...prefs,
    groups: { ...prefs.groups, [nodeId]: soloId },
    pinnedPos: { ...prefs.pinnedPos, [nodeId]: { x: Math.round(at.x), y: Math.round(at.y) } },
  };
}

/** Pins a node at a specific canvas position. */
export function pinNodeAt(prefs: WorkingSetPrefs, nodeId: string, at: { x: number; y: number }): WorkingSetPrefs {
  return {
    ...prefs,
    pinnedPos: { ...prefs.pinnedPos, [nodeId]: { x: Math.round(at.x), y: Math.round(at.y) } },
  };
}

/** Moves every node in a cluster by `(dx, dy)` and pins them at their new positions. */
export function pinClusterMoved(prefs: WorkingSetPrefs, nodeIds: string[], boxes: Map<string, NodeBox>, dx: number, dy: number): WorkingSetPrefs {
  const pinnedPos = { ...prefs.pinnedPos };
  for (const id of nodeIds) {
    const b = boxes.get(id);
    if (b) pinnedPos[id] = { x: Math.round(b.x + dx), y: Math.round(b.y + dy) };
  }
  return { ...prefs, pinnedPos };
}

/** Shelves a session or schedule hub off the working set (reversible). */
export function shelveNode(prefs: WorkingSetPrefs, id: string): WorkingSetPrefs {
  if (prefs.shelved.includes(id)) return prefs;
  return {
    ...prefs,
    shelved: [...prefs.shelved, id],
    pinnedIn: prefs.pinnedIn.filter((x) => x !== id),
  };
}

/** Pulls a quiet or shelved session onto the working set. */
export function pullInNode(prefs: WorkingSetPrefs, id: string): WorkingSetPrefs {
  return {
    ...prefs,
    shelved: prefs.shelved.filter((x) => x !== id),
    pinnedIn: prefs.pinnedIn.includes(id) ? prefs.pinnedIn : [...prefs.pinnedIn, id],
  };
}

/** Renames a cluster (locks its title). */
export function renameCluster(prefs: WorkingSetPrefs, clusterId: string, title: string): WorkingSetPrefs {
  const clean = title.trim();
  const groupNames = { ...prefs.groupNames };
  if (clean) groupNames[clusterId] = clean;
  else delete groupNames[clusterId];
  return { ...prefs, groupNames };
}

/** Clears pinned coordinates so the force layout relaxes cleanly. */
export function relaxLayout(prefs: WorkingSetPrefs): WorkingSetPrefs {
  return { ...prefs, pinnedPos: {} };
}
