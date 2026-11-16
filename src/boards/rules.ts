// What a board's rule and its columns' rules take in (model.ts `Rule`), as pure functions. Each
// thing a board can hold (a todo, a session, a worktree, a PR) is read once into a `Subject`: what
// a rule asks of it, however it's a part of it. A todo's labels go for the sessions and worktrees
// on it too, and a session's own (`Session.labels`) for it and its worktree; a todo's statuses and accounts are its sessions'. A board holds what its rule takes in
// and what was added by hand, less what was taken off (`memberTest`), and a card goes in the first
// column whose rule takes it, else in the column it was put in by hand, else in the first column
// without a rule (`columnOf`).

import { refId, type RefKind } from '../canvas/model';
import { AGENT_BAY, sessionPrs } from '../lanes';
import { agentReviewing, prReady, type PrFacts } from '../prStatus';
import { itemSection, todoPr, type TodoItem } from '../todos';
import type { Label, Lane, Pr, PrItem, Session, Todo } from '../types';
import type { SessionItem } from '../ui/SessionsView';
import { inPrLoop, takesAll, type Active, type BoardColumn, type BoardDoc, type Rule, type Stage, type StatusPick } from './model';

/** What a rule asks of one thing on the board. */
export interface Subject {
  card: string;
  ref: RefKind;
  title: string;
  /** When its work last moved on. */
  activity: number;
  stage: Stage | null;
  statuses: StatusPick[];
  labels: string[];
  repos: string[];
  accounts: string[];
}

/** A PR as the PRs view lists it, for what it says of its review and checks, where the board lists
 *  it; else as its worktree has it, which says less (`Pr`). */
function factsOf(pr: Pr, prs: readonly PrItem[]): PrFacts {
  return prs.find((p) => p.url === pr.url) ?? pr;
}

/** A PR's stage, and a worktree's by its PR, its PR's facts first (`factsOf`). A draft is under
 *  way and a merged or closed one done. An open one goes round the loop of checks, reviews and
 *  fixes (model.ts `Stage`), split by whose turn it is: Validating while the machines have it,
 *  Codealong's agent reviewing the commit it's at (prStatus.ts `agentReviewing`) or its checks
 *  still running with none failed; Ready to merge once GitHub would merge it (`prReady`: the
 *  reviews the repo requires, passing checks, no conflicts), the agent done with it; else In review,
 *  a person's turn: a review to give, feedback to answer, a failing check or a conflict to see to.
 *  It moves as its work does (a push, a check's end, a review), never with an agent's turns. */
export function prStage(pr: PrFacts | null | undefined): Stage | null {
  if (!pr) return null;
  if (pr.state === 'merged' || pr.state === 'closed') return 'done';
  if (pr.state === 'draft') return 'progress';
  if (agentReviewing(pr)) return 'validating';
  if (prReady(pr)) return 'ready';
  return pr.checks.pending > 0 && pr.checks.failed === 0 ? 'validating' : 'review';
}

/** Where a session stands: Done once it's archived (marked done, say), whatever its PR, unless it's
 *  at work or waits on you; its PR's stage while the PR it worked on (an open one first, lanes.ts
 *  `sessionPrs`) is open and not a draft; Done once that PR merged or its turn ended saying its
 *  work landed, unless it's at work or waits on you; else In progress. */
export function sessionStage(r: SessionItem, prs: PrItem[]): Exclude<Stage, 'todo'> {
  const s = r.session;
  const busy = s.status === 'working' || s.status === 'needs' || s.status === 'error';
  if (s.archived && !busy) return 'done';
  const worked = sessionPrs(prs, r.lane, s);
  const pr = worked.find((p) => p.state !== 'merged' && p.state !== 'closed') ?? worked[0];
  const stage = prStage(pr && factsOf(pr, prs));
  if (inPrLoop(stage)) return stage;
  return !busy && (pr?.state === 'merged' || s.insight?.landing === 'landed') ? 'done' : 'progress';
}

/** A todo's stage: its Todo view section (todos.ts `itemSection`), Ready and Waiting both To do,
 *  and Archived done with (an archived todo is on no board's Kanban, boardLists.ts
 *  `kanbanColumns`). One in review goes by its PR (`todoPr`, `prStage`) while that's open and not a
 *  draft; marked for review by hand without one, or with its PR merged while it isn't done, it's In
 *  review, waiting on you. `prs`: the PRs the board lists, for what they say of their checks and
 *  reviews. */
export function todoStage(item: TodoItem, todos: Todo[], prs: readonly PrItem[] = []): Stage {
  const section = itemSection(item, todos);
  if (section === 'review') {
    const pr = todoPr(item);
    const stage = prStage(pr && factsOf(pr, prs));
    return inPrLoop(stage) ? stage : 'review';
  }
  return section === 'ready' || section === 'waiting' ? 'todo' : section === 'in-progress' ? 'progress' : section === 'archived' ? 'done' : section;
}

const statusOf = (s: Session): StatusPick | null => {
  const bay = AGENT_BAY[s.status];
  return bay === 'needs' || bay === 'unread' || bay === 'working' ? bay : null;
};
const statusesOf = (ss: Session[]) => [...new Set(ss.flatMap((s) => statusOf(s) ?? []))];
const uniq = (xs: string[]) => [...new Set(xs)];

/** A repo as rules name it: its GitHub slug, else its project and name. */
export function repoOf(lane: Lane): string {
  return lane.repoSlug ?? `${lane.project}/${lane.repo}`;
}

/** `prs`: the PRs the board lists (`todoStage`). */
export function todoSubject(item: TodoItem, todos: Todo[], prs: readonly PrItem[] = []): Subject {
  const sessions = item.sessions.map((p) => p.session);
  return {
    card: refId('todo', item.todo.id),
    ref: 'todo',
    title: item.todo.title,
    activity: item.lastActivity,
    stage: todoStage(item, todos, prs),
    statuses: statusesOf(sessions),
    labels: item.todo.labels,
    repos: uniq([...item.places.map((p) => repoOf(p.lane)), ...item.sessions.map((p) => repoOf(p.lane))]),
    accounts: uniq(sessions.map((s) => s.account)),
  };
}

/** A session's own labels by this Mac's ids, as rules name them: one of this Mac's is itself, even
 *  where another here has its name too; one on another device wears that Mac's labels, so each of
 *  those goes by the one of its name here (`labels`), else by its own id. */
export function ownLabels(s: Pick<Session, 'labels'>, labels: readonly Label[]): string[] {
  const here = (id: string) => labels.some((x) => x.id === id);
  return (s.labels ?? []).map((l) => (here(l.id) ? l.id : (labels.find((x) => x.name.toLowerCase() === l.name.toLowerCase())?.id ?? l.id)));
}

/** `on`: the todos it's linked to, whose labels go with its own; `labels`, this Mac's
 *  (`ownLabels`). */
export function sessionSubject(r: SessionItem, prs: PrItem[], on: Todo[], labels: readonly Label[] = []): Subject {
  const s = r.session;
  return {
    card: refId('session', s.key),
    ref: 'session',
    title: s.title,
    activity: s.lastActivity,
    stage: sessionStage(r, prs),
    statuses: statusesOf([s]),
    labels: uniq([...on.flatMap((t) => t.labels), ...ownLabels(s, labels)]),
    repos: [repoOf(r.lane)],
    accounts: [s.account],
  };
}

/** `on`: the todos linked to it or to a session in it, whose labels go with its sessions' own;
 *  `labels`, this Mac's (`ownLabels`); `prs`, the PRs the board lists (`prStage`). */
export function laneSubject(lane: Lane, on: Todo[], labels: readonly Label[] = [], prs: readonly PrItem[] = []): Subject {
  return {
    card: refId('worktree', lane.id),
    ref: 'worktree',
    title: lane.callsign,
    activity: lane.lastActivity,
    stage: prStage(lane.pr && factsOf(lane.pr, prs)),
    statuses: statusesOf(lane.sessions),
    labels: uniq([...on.flatMap((t) => t.labels), ...lane.sessions.flatMap((s) => ownLabels(s, labels))]),
    repos: [repoOf(lane)],
    accounts: uniq(lane.sessions.map((s) => s.account)),
  };
}

/** `sessions`: the sessions that worked on it. */
export function prSubject(pr: PrItem, sessions: Session[]): Subject {
  return {
    card: refId('pr', pr.key),
    ref: 'pr',
    title: `#${pr.number} ${pr.title}`,
    activity: pr.updatedAt,
    stage: prStage(pr),
    statuses: statusesOf(sessions),
    labels: [],
    repos: [pr.repoSlug],
    accounts: uniq(sessions.map((s) => s.account)),
  };
}

export interface SubjectsInput {
  todoItems: TodoItem[];
  todos: Todo[];
  sessionItems: SessionItem[];
  lanes: Lane[];
  prs: PrItem[];
  /** The labels todos wear on this Mac, which rules name: another device's sessions' go by them
   *  (`ownLabels`). */
  labels?: readonly Label[];
}

/** Everything a board in the project can hold, by card id. */
export function subjectsOf({ todoItems, todos, sessionItems, lanes, prs, labels = [] }: SubjectsInput): Map<string, Subject> {
  const bySession = new Map<string, Todo[]>();
  const byLane = new Map<string, Todo[]>();
  const add = (m: Map<string, Todo[]>, k: string, t: Todo) => {
    const list = m.get(k) ?? [];
    if (!list.includes(t)) list.push(t);
    m.set(k, list);
  };
  const homeOf = new Map(sessionItems.map((r) => [r.session.key, r.lane.id]));
  for (const t of todos) {
    for (const k of t.sessions) {
      add(bySession, k, t);
      const home = homeOf.get(k);
      if (home) add(byLane, home, t);
    }
    for (const id of t.worktrees) add(byLane, id, t);
  }
  const out = new Map<string, Subject>();
  const put = (s: Subject) => out.set(s.card, s);
  todoItems.forEach((r) => put(todoSubject(r, todos, prs)));
  sessionItems.forEach((r) => put(sessionSubject(r, prs, bySession.get(r.session.key) ?? [], labels)));
  lanes.forEach((l) => put(laneSubject(l, byLane.get(l.id) ?? [], labels, prs)));
  const sessionByKey = new Map(sessionItems.map((r) => [r.session.key, r.session]));
  prs.forEach((p) => put(prSubject(p, p.sessions.flatMap((x) => sessionByKey.get(x.key) ?? []))));
  return out;
}

const DAY = 86_400_000;
const ACTIVE_MS: Record<Active, number> = { '1d': DAY, '3d': 3 * DAY, '7d': 7 * DAY, '30d': 30 * DAY };

const anyOf = <T>(want: readonly T[] | undefined, has: readonly T[]) => !want?.length || want.some((w) => has.includes(w));

/** Whether a rule takes the thing in: every part it has holds. */
export function matches(rule: Rule, s: Subject, now: number): boolean {
  const lower = (xs: readonly string[]) => xs.map((x) => x.toLowerCase());
  return (
    anyOf(rule.kinds, [s.ref]) &&
    anyOf(rule.stages, s.stage ? [s.stage] : []) &&
    anyOf(rule.statuses, s.statuses) &&
    anyOf(rule.labels, s.labels) &&
    anyOf(rule.repos && lower(rule.repos), lower(s.repos)) &&
    anyOf(rule.accounts, s.accounts) &&
    (!rule.active || s.activity >= now - ACTIVE_MS[rule.active]) &&
    (!rule.text?.trim() || rule.text.toLowerCase().split(/\s+/).filter(Boolean).every((w) => s.title.toLowerCase().includes(w)))
  );
}

/** Whether a card is on the board: added by hand, or taken in by its rule, and not taken off. */
export function memberTest(board: BoardDoc, subjects: Map<string, Subject>, now: number): (card: string) => boolean {
  const removed = new Set(board.removed);
  if (takesAll(board.rule) && removed.size === 0) return () => true;
  const on = new Set(board.added);
  if (board.rule) for (const s of subjects.values()) if (matches(board.rule, s, now)) on.add(s.card);
  return (card) => on.has(card) && !removed.has(card);
}

/** The column a card goes in: the first whose rule takes it, else the one it was put in by hand
 *  (which has no rule), else the first without a rule; none when every column has a rule and none
 *  takes it. */
export function columnOf(s: Subject, columns: BoardColumn[], placed: Record<string, string> | undefined, now: number): string | null {
  for (const c of columns) if (c.rule && matches(c.rule, s, now)) return c.id;
  const hand = placed?.[s.card];
  if (hand && columns.some((c) => c.id === hand && !c.rule)) return hand;
  return columns.find((c) => !c.rule)?.id ?? null;
}
