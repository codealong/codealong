// What the quick panel lists, and how it names shortcuts. With nothing typed it lists the
// sessions that need you, then the ones whose turn ended, snoozed ones left out until their time
// as on the floating bar; with nothing waiting, the ones working. Under them go the first few
// todos ready to start, then the rest of the sessions started in the last day, which ↓ reaches
// past the rest. Typed words find sessions, then todos, then PRs: every word has to be in what
// describes them, or, for a session, in what was said in it (the engine's text search,
// search.rs). Under a prompt, Send to lists every session it could go to, Jev's picks first
// (`sendTargets`), its own box finding one while the prompt waits. Kept free of React so the
// rules have tests.

import { agoPhrase, middleTruncate } from './format';
import { heldForAgent } from './prStatus';
import { branchLabel, byStatusTime, sessionHomes, snoozedApart, unattended } from './lanes';
import { inQueueOrder, statusWaits, type Reads } from './queue';
import { itemSection, sectionOf, todoItem, type TodoItem, type TodoSectionId } from './todos';
import { prFlag } from './prStatus';
import type { Codec } from './stored';
import { platform } from './platform';
import type { Attachment, Board, DeviceFound, DeviceView, FolderLook, Lane, PrItem, PrState, RoutePick, Session, TextHit, Todo } from './types';

export const ALL_PROJECTS = 'All projects';
/** What the panel's empty box says, and the box in the main window's title bar that opens it. */
export const QUICK_PLACEHOLDER = 'Find a session, or describe new work…';
/** What the box says in Send to, while it finds the session what's typed goes to. */
export const SEND_TO_PLACEHOLDER = 'Find the session to send this to…';
/** Codealong's chats, as the worktree picker offers them: a chat of its own, in no project. */
export const NO_PROJECT = 'No project';

/** Whether a worktree (or a floating bar pill, which carries its worktree's project) is in the
 *  project the panel shows. All projects has every project's, Elsewhere's among them, as the main
 *  window and the bar do. */
export function inProject(lane: Pick<Lane, 'project'>, project: string): boolean {
  return project === ALL_PROJECTS || lane.project === project;
}

export type RowGroup = 'suggested' | 'skills' | 'needs' | 'reviews' | 'unread' | 'working' | 'found' | 'sendto' | 'todos' | 'prs' | 'ready' | 'recent';

/** Where a session goes in the empty panel's list by its status, as on the floating bar: errors
 *  wait on you like questions do. A helper or a headless run waits there only while it needs
 *  you, and one you snoozed not until its time, though it shows with Working while it runs
 *  (barfeed.rs). */
function groupOf(s: Session): 'needs' | 'unread' | 'working' | null {
  if (unattended(s) || snoozedApart(s)) return null;
  if (s.status === 'needs' || s.status === 'error') return 'needs';
  if (s.status === 'unread' || s.status === 'working') return s.status;
  return null;
}

export interface SessionRow {
  kind: 'session';
  key: string;
  group: RowGroup;
  lane: Lane;
  session: Session;
  /** Where the typed words are in the title, as [start, end) pairs, for underlining. */
  marks: [number, number][];
  /** Found by what was said rather than what describes it (`saidLine`). */
  said?: SaidLine;
  /** How likely Jev thinks it is that what's typed is for this session, in Send to
   *  (`sendTargets`). */
  p?: number;
}

/** What a session found by what was said shows under its title: a line of what matched, whether
 *  you wrote it, and where the words are in it. */
export interface SaidLine {
  text: string;
  you: boolean;
  marks: [number, number][];
}

export function saidLine(hit: TextHit, words: string[]): SaidLine {
  return { text: hit.snippet, you: hit.you, marks: titleMarks(hit.snippet, words) };
}

/** A PR waiting on your review, in the empty panel's list among the sessions, keyed by the PR's
 *  key (`owner/name#12`), which no session's key looks like. ⏎ on it reviews it in the waiting
 *  window. */
export interface ReviewRow {
  kind: 'review';
  key: string;
  group: 'reviews';
  pr: PrItem;
  marks: [number, number][];
}

/** A row the waiting window works through: a session, or a review asked of you. */
export type QueueRow = SessionRow | ReviewRow;

/** When a review began waiting on you, as the queue keeps it for the ones dealt with (queue.ts
 *  `Handled`): the latest request to you, else the PR's last update. */
export const reviewSince = (pr: PrItem) => pr.requestedAt ?? pr.updatedAt;

/** The open PRs that wait on your review, your snoozed ones and the ones Codealong's agent is
 *  still reviewing left out, the longest wait first, as the floating bar lists them (barfeed.rs). */
export function reviewRows(board: Board, project: string): ReviewRow[] {
  return (board.prs ?? [])
    .filter((p) => p.reviewRequested && p.state !== 'merged' && p.state !== 'closed' && !p.snoozedUntil && !heldForAgent(p) && (project === ALL_PROJECTS || p.project === project))
    .sort((a, b) => reviewSince(a) - reviewSince(b) || a.key.localeCompare(b.key))
    .map((pr) => ({ kind: 'review' as const, key: pr.key, group: 'reviews' as const, pr, marks: [] }));
}

/** How many rows the panel lists at most: ⌘1 to ⌘9 reach them all. */
export const MAX_ROWS = 9;
/** How many todos ready to start the empty panel lists under the sessions. */
export const READY_TODOS = 3;
/** How far back the empty panel's recent sessions go, and how many it lists at most. */
export const RECENT_MS = 24 * 60 * 60 * 1000;
export const RECENT_SESSIONS = 20;
/** A search lists this many sessions and this many todos at most, so both fit. */
export const FOUND_SESSIONS = 6;
export const FOUND_TODOS = 5;
export const FOUND_PRS = 4;

/** Which sessions a search looks through: those that aren't archived, archived ones too, or
 *  only archived ones. */
export type Archived = 'without' | 'with' | 'only';

/** Every session once, listed under the worktree it runs in (not as a visitor elsewhere), with
 *  archived ones left out unless `archived` says otherwise. */
function sessionsIn(board: Board, project: string, archived: Archived = 'without'): { lane: Lane; session: Session }[] {
  const out: { lane: Lane; session: Session }[] = [];
  for (const lane of board.lanes) {
    if (!inProject(lane, project)) continue;
    for (const session of lane.sessions) {
      if (session.visitor || (archived === 'without' && session.archived) || (archived === 'only' && !session.archived)) continue;
      out.push({ lane, session });
    }
  }
  return out;
}

/** When a session's status last changed, as the queue keeps it for the ones dealt with
 *  (queue.ts `Handled`). */
export const statusTime = (s: Session) => s.statusSince ?? s.lastActivity;

/** How the queue (queue.ts) reads the waiting window's rows, and the empty panel's: a session's
 *  row by its session, a review's by its PR. */
export const QUEUE_READS: Reads<QueueRow> = {
  keyOf: (r) => (r.kind === 'session' ? r.session.key : r.pr.key),
  sinceOf: (r) => (r.kind === 'session' ? statusTime(r.session) : reviewSince(r.pr)),
  waitsOf: (r) => (r.kind === 'session' ? statusWaits(r.session.status) : 'reviews'),
  titleOf: (r) => (r.kind === 'session' ? r.session.title : `#${r.pr.number} ${r.pr.title}`),
};

/** The empty panel's list: what waits on you in the order the waiting window works through it
 *  (queue.ts `byPlace`), sessions that need you, then the PRs waiting on your review, then the
 *  sessions whose turn ended, each the longest wait first, as on the floating bar and in the
 *  notch (bar.ts `NOTCH_ORDER`); with none of those, the ones working, the longest running first.
 *  Snoozed ones are left out until their time (`groupOf`), so a recent one is in `recentRows`.
 *  `counts` has every group's full size, for the headings. */
export function attentionRows(board: Board, project: string, limit = MAX_ROWS): { rows: QueueRow[]; counts: Record<'needs' | 'reviews' | 'unread' | 'working', number> } {
  const sessions = sessionsIn(board, project)
    .flatMap(({ lane, session }) => {
      const group = groupOf(session);
      return group ? [{ kind: 'session' as const, key: session.key, group, lane, session, marks: [] }] : [];
    })
    .sort((a, b) => ORDER.indexOf(a.group) - ORDER.indexOf(b.group) || byStatusTime(a.session, b.session) || a.key.localeCompare(b.key));
  const reviews = reviewRows(board, project);
  const counts = { needs: 0, reviews: reviews.length, unread: 0, working: 0 };
  for (const r of sessions) counts[r.group as 'needs' | 'unread' | 'working'] += 1;
  const waiting = inQueueOrder<QueueRow>([...sessions.filter((r) => r.group !== 'working'), ...reviews], QUEUE_READS);
  return { rows: (waiting.length > 0 ? waiting : sessions).slice(0, limit), counts };
}

const ORDER: RowGroup[] = ['needs', 'unread', 'working'];

/**
 * What the waiting window works through (Waiting.tsx): every session in the project that needs
 * you, then the PRs waiting on your review, then the sessions whose turn ended, each the longest
 * wait first, as the empty panel's list has them. queue.ts has how it goes from one to the next.
 */
export function queueRows(board: Board, project: string): QueueRow[] {
  return attentionRows(board, project, Infinity).rows.filter((r) => r.group !== 'working');
}

/**
 * The empty panel's last list: sessions started in the last day (by their last activity when the
 * start isn't known) that the lists above don’t show, the newest status first, as their ages read. `count`
 * is how many there are in all, for the heading.
 */
export function recentRows(board: Board, project: string, shown: Set<string>, now: number, limit = RECENT_SESSIONS): { rows: SessionRow[]; count: number } {
  const recent = sessionsIn(board, project)
    .filter(({ session: s }) => !shown.has(s.key) && !unattended(s) && now - (s.startedAt ?? s.lastActivity) <= RECENT_MS)
    .sort((a, b) => statusTime(b.session) - statusTime(a.session) || a.session.key.localeCompare(b.session.key));
  const rows = recent.slice(0, limit).map(({ lane, session }): SessionRow => ({ kind: 'session', key: session.key, group: 'recent', lane, session, marks: [] }));
  return { rows, count: recent.length };
}

/** The words to look for, lowercased. */
export function queryWords(query: string): string[] {
  return query.toLowerCase().split(/\s+/).filter(Boolean);
}

/**
 * Where `word` first appears in `text` (lowercased), at `from` or after, or -1. Among other words,
 * a word of one or two letters counts only whole: "class c" shouldn't find every session with a c
 * in it (every `claude` account has one). Alone it's a search as you type and matches anywhere.
 * The engine's text search has the same rule (`search::has_word`).
 */
export function wordAt(text: string, word: string, of: number, from = 0): number {
  if (of < 2 || word.length > 2) return text.indexOf(word, from);
  const edge = (c: string | undefined) => !c || !/[\p{L}\p{N}]/u.test(c);
  for (let i = text.indexOf(word, from); i >= 0; i = text.indexOf(word, i + 1)) {
    if (edge(text[i - 1]) && edge(text[i + word.length])) return i;
  }
  return -1;
}

/** Whether `hay` has every word, by `wordAt`'s rule. */
export function hasWords(hay: string, words: string[]): boolean {
  return words.every((w) => wordAt(hay, w, words.length) >= 0);
}

/** Where each word first appears in the title, merged where they overlap. */
export function titleMarks(title: string, words: string[]): [number, number][] {
  const t = title.toLowerCase();
  const spans = words
    .map((w) => [wordAt(t, w, words.length), w.length] as const)
    .filter(([at]) => at >= 0)
    .map(([at, n]) => [at, at + n] as [number, number])
    .sort((a, b) => a[0] - b[0]);
  const out: [number, number][] = [];
  for (const s of spans) {
    const last = out[out.length - 1];
    if (last && s[0] <= last[1]) last[1] = Math.max(last[1], s[1]);
    else out.push([s[0], s[1]]);
  }
  return out;
}

/**
 * How far down a search puts a session for how closely the words name it (`findSessions`), in
 * doublings of its age: its title or name is the words; its title or name has them all; the rest
 * of what describes it does (account, worktree, repo, branch, project, PR); only what was said in
 * it does. A title with the words stays ahead of a repo or branch with them until it's four times
 * older, and ahead of a session that only mentions them until it's some sixty times older: a title
 * from two days ago comes before a session working now that mentions the words, and one from ten
 * days ago after a mention an hour old. What was said is weak: a long session mentions most words.
 */
const CLOSENESS = { exact: 0, named: 3, described: 5, said: 9 };
/** What a line of what was said scores at best (search.rs `BEST_SCORE`): the words as typed, by
 *  you. A session found by a weaker line goes a quarter of a doubling further down for each point
 *  short of it. The engine keeps the sessions found by what was said that this puts first, by the
 *  same weights (search.rs `kept_first`), so a change to them goes there too. */
const SAID_BEST = 7;
const HOUR = 60 * 60 * 1000;

/** Where a search puts a session, the lowest first: how closely the words name it (`CLOSENESS`),
 *  then how many times its age has doubled, in hours since it was last active (an hour old is one,
 *  three hours two, a day about five, a week seven). An archived one counts one more: you mostly
 *  look for what you still work on. */
function searchCost(closeness: number, s: Session, now: number): number {
  return closeness + (s.archived ? 1 : 0) + Math.log2(1 + Math.max(0, now - s.lastActivity) / HOUR);
}

/** How many doublings further down a search puts a session in another project than the one
 *  picked: as if it were eight times older. A title in the project you picked comes before the
 *  same title elsewhere, but a session elsewhere that the words name still comes before one in
 *  the project that only mentions them. */
const ANOTHER_PROJECT = 3;

/** The shortest search that's sent to the text search: fewer letters match nearly everything. */
export const TEXT_SEARCH_MIN = 3;

/**
 * Sessions with every typed word somewhere in what describes them, or in what was said in them
 * (`said`, the text search's hits by session key), the likeliest first. How closely the words name
 * each counts, in the order `codealong sessions find` ranks them (cli.rs `Match`): a title or name
 * that is the words, then a title or name that has them all, then the rest of what describes it,
 * then what was said, the words as typed best there. How long ago it was last active counts too,
 * enough that a recent session that matches less closely comes before an old one that matches
 * better (`CLOSENESS` says how far): the session you look for is most often one you worked in
 * lately, and a common word names hundreds of old ones. Archived sessions are left out unless
 * `archived` asks for them, and then count as older than they are. A project picked doesn't hide
 * the others' sessions from a search, since the one you look for may be in another: they come
 * after the project's own as older ones would (`ANOTHER_PROJECT`), and each row says its project.
 */
export function findSessions(board: Board, project: string, query: string, now: number, limit = MAX_ROWS, said: ReadonlyMap<string, TextHit> = new Map(), archived: Archived = 'without'): SessionRow[] {
  const words = queryWords(query);
  if (words.length === 0) return [];
  const asked = words.join(' ');
  const others = project === ALL_PROJECTS ? [] : sessionsIn(board, ALL_PROJECTS, archived).filter(({ lane }) => !inProject(lane, project));
  const found = [...sessionsIn(board, project, archived), ...others].flatMap(({ lane, session: s }) => {
    const away = inProject(lane, project) ? 0 : ANOTHER_PROJECT;
    const cost = (closeness: number) => away + searchCost(closeness, s, now);
    // Marks only for a session that matches: a long prompt has hundreds of words, and looking for
    // each in every session's title was most of what a key cost.
    const row = (): SessionRow => ({ kind: 'session', key: s.key, group: 'found', lane, session: s, marks: titleMarks(s.title, words) });
    // Each name on its own: words split between a title and a name don't name the session.
    const names = s.name ? [s.title, s.name] : [s.title];
    const named = names.filter((n) => hasWords(n.toLowerCase(), words));
    if (named.length > 0) return [{ row: row(), cost: cost(named.some((n) => queryWords(n).join(' ') === asked) ? CLOSENESS.exact : CLOSENESS.named) }];
    const pr = lane.pr ? `#${lane.pr.number} ${lane.pr.title}` : '';
    const hay = [...names, s.account, lane.callsign, lane.repo, lane.branch ?? '', s.branch ?? '', lane.project, pr].join('\n').toLowerCase();
    if (hasWords(hay, words)) return [{ row: row(), cost: cost(CLOSENESS.described) }];
    const hit = said.get(s.key);
    if (!hit) return [];
    return [{ row: { ...row(), said: saidLine(hit, words) }, cost: cost(CLOSENESS.said + (SAID_BEST - Math.min(hit.score ?? 0, SAID_BEST)) / 4) }];
  });
  return found
    .sort((a, b) => a.cost - b.cost || b.row.session.lastActivity - a.row.session.lastActivity || a.row.key.localeCompare(b.row.key))
    .slice(0, limit)
    .map((f) => f.row);
}

/** How many sessions Send to lists at most; words typed in its box find the rest. */
export const SEND_TO = 40;

/**
 * The sessions Send to lists, for sending what's typed to one Jev didn't suggest. With nothing
 * typed in its box: the ones Jev weighed (`picks`, its answer's sessions with how likely each
 * is), in Jev's order, then every other session in the project, the one you last prompted first,
 * so the list holds still while agents work. With words typed: the sessions they find
 * (`findSessions`, by what was said in them too, and in other projects after the project's own),
 * Jev's first in its order, the rest in theirs.
 * Helpers and headless runs are left out while they don't need you, as the empty panel leaves
 * them out: their agent or script talks to them, not you. Archived ones are left out unless
 * `archived` asks for them: then they come last with nothing typed, and with words where
 * `findSessions` puts them. Jev doesn't pick an archived session (route.rs), and one archived
 * since Jev answered isn't taken for a pick. `count` is how many there are in all.
 */
export function sendTargets(board: Board, project: string, words: string, picks: RoutePick[], now: number, said: ReadonlyMap<string, TextHit> = new Map(), archived: Archived = 'without', limit = SEND_TO): { rows: SessionRow[]; count: number } {
  const likely = new Map(picks.map((x) => [x.id, x.p]));
  const rank = new Map(picks.map((x, i) => [x.id, i]));
  // The panel keeps Jev's answer while the board moves on: a pick archived since isn't one.
  const at = (r: SessionRow) => (r.session.archived ? Infinity : (rank.get(r.session.key) ?? Infinity));
  const found =
    queryWords(words).length > 0
      ? findSessions(board, project, words, now, Infinity, said, archived)
      : sessionsIn(board, project, archived)
          .sort((a, b) => Number(a.session.archived) - Number(b.session.archived) || b.session.lastPromptAt - a.session.lastPromptAt || a.session.key.localeCompare(b.session.key))
          .map(({ lane, session }): SessionRow => ({ kind: 'session', key: session.key, group: 'sendto', lane, session, marks: [] }));
  const all = found
    .filter((r) => !unattended(r.session))
    .map((r, i) => ({ r, i }))
    .sort((a, b) => at(a.r) - at(b.r) || a.i - b.i)
    .map(({ r }): SessionRow => {
      const p = r.session.archived ? undefined : likely.get(r.session.key);
      return { ...r, group: 'sendto', ...(p != null ? { p } : {}) };
    });
  return { rows: all.slice(0, limit), count: all.length };
}

/**
 * What the last text search found that is still found by its own line: hits whose line has every
 * word typed now. They stay while the search for the new words runs, so a session found by what
 * was said doesn't drop out of the list with each key and come back when the search answers. A
 * hit whose line lacks a word waits for that answer.
 */
export function stillSaid(hits: TextHit[], words: string[]): TextHit[] {
  return hits.filter((h) => {
    return hasWords(h.snippet.toLowerCase(), words);
  });
}

/** What your other devices found, as one list: their hits (keyed as their sessions are here,
 *  `@<short>:`), and whether any is still reading what its sessions said. A device that couldn't
 *  be asked adds nothing: its sessions are still found by what describes them. */
export function deviceHits(found: DeviceFound[]): { hits: TextHit[]; building: boolean } {
  return { hits: found.flatMap((d) => d.found?.hits ?? []), building: found.some((d) => !!d.found?.building) };
}

/** A todo a search found, or one ready to start under the empty panel's sessions. */
export interface TodoRow {
  kind: 'todo';
  key: string;
  group: 'todos' | 'ready';
  item: TodoItem;
  /** Waiting on something, by hand or on another todo that isn't done. */
  waiting: boolean;
  marks: [number, number][];
}

const SECTION_RANK: Record<TodoSectionId, number> = { 'in-progress': 0, review: 1, ready: 2, waiting: 3, done: 4, archived: 5 };

/**
 * Todos with every typed word in their title, description, labels or project, or in the
 * worktrees and sessions linked to them, best first: those whose title has them all, then In
 * progress, In review, Ready, Waiting and Done, then the latest activity. In a project, only its own todos.
 * Archived ones aren't found: they're in the Todo view's Archived fold alone.
 */
export function findTodos(board: Board, project: string, query: string, limit = FOUND_TODOS): TodoRow[] {
  const words = queryWords(query);
  if (words.length === 0) return [];
  const todos = board.todos ?? [];
  const laneById = new Map(board.lanes.map((l) => [l.id, l]));
  const homes = sessionHomes(board.lanes);
  const labelById = new Map((board.labels ?? []).map((l) => [l.id, l]));
  const found = todos
    .filter((t) => !t.archivedAt && (project === ALL_PROJECTS || t.project === project))
    .flatMap((t) => {
      const item = todoItem(t, laneById, homes, labelById);
      const hay = [
        t.title,
        t.description,
        t.project,
        ...item.labels.map((l) => l.name),
        ...item.places.map((p) => `${p.lane.callsign} ${p.lane.branch ?? ''}`),
        ...item.sessions.map((x) => `${x.session.account} ${x.session.title}`),
      ]
        .join('\n')
        .toLowerCase();
      if (!hasWords(hay, words)) return [];
      const section = itemSection(item, todos);
      const title = t.title.toLowerCase();
      const row: TodoRow = { kind: 'todo', key: `todo:${t.id}`, group: 'todos', item, waiting: section === 'waiting', marks: titleMarks(t.title, words) };
      return [{ row, inTitle: hasWords(title, words), rank: SECTION_RANK[section] }];
    });
  return found
    .sort((a, b) => Number(b.inTitle) - Number(a.inTitle) || a.rank - b.rank || b.row.item.lastActivity - a.row.item.lastActivity || a.row.key.localeCompare(b.row.key))
    .slice(0, limit)
    .map((f) => f.row);
}

/** A PR a search found. */
export interface PrRow {
  kind: 'pr';
  key: string;
  group: 'prs';
  pr: PrItem;
  marks: [number, number][];
}

const PR_STATE_RANK: Record<PrState, number> = { open: 0, draft: 0, approved: 0, changes: 0, merged: 1, closed: 2 };

/**
 * PRs with every typed word in their number (`#123` or `123`), title, repo, branch, author or
 * project, best first: those whose title has them all, then open ones (your move first), merged,
 * closed, then the latest activity. In a project, only its own, as with sessions and todos.
 */
export function findPrs(board: Board, project: string, query: string, limit = FOUND_PRS): PrRow[] {
  const words = queryWords(query);
  if (words.length === 0) return [];
  const found = (board.prs ?? [])
    .filter((p) => project === ALL_PROJECTS || p.project === project)
    .flatMap((p) => {
      const hay = [`#${p.number}`, p.title, p.repo, p.repoSlug, p.head, p.author ?? '', p.project].join('\n').toLowerCase();
      if (!hasWords(hay, words)) return [];
      const title = p.title.toLowerCase();
      const row: PrRow = { kind: 'pr', key: `pr:${p.key}`, group: 'prs', pr: p, marks: titleMarks(p.title, words) };
      return [{ row, inTitle: hasWords(title, words), rank: PR_STATE_RANK[p.state] * 2 + (prFlag(p) ? 0 : 1) }];
    });
  return found
    .sort((a, b) => Number(b.inTitle) - Number(a.inTitle) || a.rank - b.rank || b.row.pr.updatedAt - a.row.pr.updatedAt || a.row.key.localeCompare(b.row.key))
    .slice(0, limit)
    .map((f) => f.row);
}

/**
 * The empty panel's todos: the first few ready to start (not started, and waiting on nothing), in
 * your order, as the Todo view lists them under Ready. In a project, only its own. `count` is how
 * many are ready in all, for the heading.
 */
export function readyTodos(board: Board, project: string, limit = READY_TODOS): { rows: TodoRow[]; count: number } {
  const todos = board.todos ?? [];
  const ready = todos.filter((t) => (project === ALL_PROJECTS || t.project === project) && sectionOf(t, todos) === 'ready');
  const laneById = new Map(board.lanes.map((l) => [l.id, l]));
  const homes = sessionHomes(board.lanes);
  const labelById = new Map((board.labels ?? []).map((l) => [l.id, l]));
  const rows = ready.slice(0, limit).map((t): TodoRow => ({ kind: 'todo', key: `todo:${t.id}`, group: 'ready', item: todoItem(t, laneById, homes, labelById), waiting: false, marks: [] }));
  return { rows, count: ready.length };
}

/** A heading of the worktree menu and the worktrees under it. `more`: a repo's worktrees that are
 *  rarely started in, which the menus fold away (Picker.tsx `folded`). */
export interface LaneGroup {
  label: string;
  lanes: Lane[];
  more?: boolean;
}

/** Where a new session can start, for the worktree menus: pinned worktrees first, in your order,
 *  then the rest by repo, the most recently active repo first. In each repo its main checkout
 *  leads, then its reusable worktrees, the most recently active first: the main checkout can be
 *  on any branch, so its place is what finds it. A repo's other worktrees, each kept for one
 *  piece of work, are seldom started in again: they come after every repo's, as groups marked
 *  `more`. A repo whose main checkout isn't listed keeps them with the rest, having nothing else
 *  to start in. */
export function laneGroups(lanes: Lane[], pinned: string[]): LaneGroup[] {
  const byId = new Map(lanes.map((l) => [l.id, l]));
  const pins = pinned.flatMap((id) => byId.get(id) ?? []);
  const pinnedIds = new Set(pins.map((l) => l.id));
  const projects = new Set(lanes.map((l) => l.project)).size;
  const labelOf = (l: Lane) => (projects > 1 ? `${l.project} · ${l.repo}` : l.repo);
  const withMain = new Set(lanes.filter((l) => l.origin === 'main').map(labelOf));
  const groups = new Map<string, Lane[]>();
  for (const l of lanes) {
    if (pinnedIds.has(l.id)) continue;
    groups.set(labelOf(l), [...(groups.get(labelOf(l)) ?? []), l]);
  }
  const latest = (ls: Lane[]) => Math.max(...ls.map((l) => l.lastActivity));
  const repos = [...groups.entries()]
    .sort(([, a], [, b]) => latest(b) - latest(a))
    .map(([label, ls]) => {
      const sorted = [...ls].sort((a, b) => Number(b.origin === 'main') - Number(a.origin === 'main') || b.lastActivity - a.lastActivity);
      const often = (l: Lane) => !withMain.has(label) || l.origin === 'main' || !!l.reuse;
      return { label, lanes: sorted.filter(often), rest: sorted.filter((l) => !often(l)) };
    });
  return [
    ...(pins.length > 0 ? [{ label: 'Pinned', lanes: pins }] : []),
    ...repos.filter((r) => r.lanes.length > 0).map((r) => ({ label: r.label, lanes: r.lanes })),
    ...repos.filter((r) => r.rest.length > 0).map((r) => ({ label: r.label, lanes: r.rest, more: true })),
  ];
}

/** What's typed in the worktree menu names a folder: from your home folder or the top of the disk. */
export function isFolderPath(text: string): boolean {
  const t = text.trim();
  return t === '~' || t.startsWith('~/') || t.startsWith('/');
}

/** Where a folder typed in the worktree menu starts from: the folder most of this Mac's main
 *  checkouts are in, `~/` without any. Written whole: the page doesn't know which folder is your
 *  home (`tildePath` takes any `/Users/<name>` for it), and the engine writes it with `~`. */
export function folderStart(lanes: readonly Pick<Lane, 'path' | 'origin' | 'device'>[]): string {
  const counts = new Map<string, number>();
  for (const l of lanes) {
    const parent = l.path.slice(0, l.path.lastIndexOf('/'));
    if (l.origin === 'main' && !l.device && parent) counts.set(parent, (counts.get(parent) ?? 0) + 1);
  }
  const most = [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))[0]?.[0];
  return most ? `${most}/` : '~/';
}

/** What a folder typed in the worktree menu offers, from what the engine found there
 *  (`lookFolder`): the worktree the board has there; else adding it to a project of its own and
 *  to the one the panel is on, or to the one config.toml lists it in; or why it can't be added.
 *  Then the folders in it whose names start with what's typed, to go on typing from; those come
 *  first while what's typed isn't a folder yet, as ⏎ picks the first row and a name half typed
 *  shouldn't be made. */
export type FolderChoice =
  | { kind: 'worktree'; lane: string }
  | { kind: 'add'; project: string | null; label: string; detail: string }
  | { kind: 'refused'; why: string }
  | { kind: 'into'; path: string; label: string };

/** `look`'s choices in the worktree menu, the panel on project `current`, with the board's
 *  `projects`. A project of its own is named after the folder, or is the project of that name
 *  when there's one (config.rs `with_folder`). */
export function folderChoices(look: FolderLook, current: string, projects: readonly string[]): FolderChoice[] {
  const into = look.completions.map((path): FolderChoice => ({ kind: 'into', path, label: `${path.slice(path.lastIndexOf('/') + 1)}/` }));
  if (look.refused) return [{ kind: 'refused', why: look.refused }, ...into];
  if (look.lane) return [{ kind: 'worktree', lane: look.lane }, ...into];
  const what = look.exists ? `Add ${look.shown}` : `New folder ${look.shown}`;
  const add = (project: string | null, where: string): FolderChoice => ({ kind: 'add', project, label: look.name, detail: `${what} · ${where}` });
  const own = add(null, projects.includes(look.name) ? `in ${look.name}` : 'a project of its own');
  // Elsewhere is where folders outside your projects go: none is added to it.
  const real = ![ALL_PROJECTS, NO_PROJECT, 'Elsewhere', look.name].includes(current) && projects.includes(current);
  const adds = look.project ? [add(look.project, `in ${look.project}`)] : [own, ...(real ? [add(current, `in ${current}`)] : [])];
  return !look.exists && into.length > 0 ? [...into, ...adds] : [...adds, ...into];
}

/** A worktree's branch, when it says more than its name. */
export function laneBranch(l: Lane): string | null {
  const branch = branchLabel(l);
  return branch && branch !== l.callsign ? middleTruncate(branch, 44) : null;
}

/** Needs you and Your turn per project, for the project menu. */
export function projectCounts(board: Board, projects: string[]): Record<string, { needs: number; unread: number }> {
  const out: Record<string, { needs: number; unread: number }> = {};
  for (const name of projects) {
    const c = { needs: 0, unread: 0 };
    for (const { session } of sessionsIn(board, name)) {
      const g = groupOf(session);
      if (g === 'needs' || g === 'unread') c[g] += 1;
    }
    out[name] = c;
  }
  return out;
}

/* ---- Shortcuts ---- */

/** Modifiers in the order macOS writes them, with each one's sign there and its name on Linux.
 *  These are keys as they are, not the windows' chords (platform.ts): a global shortcut is the
 *  desktop's, where `Cmd` is Super, as the shortcut plugin reads it. */
const MODS: [string, string, string][] = [
  ['Ctrl', '⌃', 'Ctrl'],
  ['Option', '⌥', 'Alt'],
  ['Shift', '⇧', 'Shift'],
  ['Cmd', '⌘', 'Super'],
];

const MOD_ALIASES: Record<string, string> = {
  ctrl: 'Ctrl',
  control: 'Ctrl',
  option: 'Option',
  alt: 'Option',
  shift: 'Shift',
  cmd: 'Cmd',
  command: 'Cmd',
  super: 'Cmd',
  cmdorctrl: 'Cmd',
  commandorcontrol: 'Cmd',
};

/** A modifier's name in config.toml as `MODS` has it. `CmdOrCtrl` is Ctrl off macOS, as the
 *  shortcut plugin reads it. */
function modOf(part: string): string | undefined {
  const m = MOD_ALIASES[part.toLowerCase()];
  return m === 'Cmd' && platform() === 'linux' && /^c(md|ommand)orc(trl|ontrol)$/i.test(part) ? 'Ctrl' : m;
}

/** Keys whose names read better as symbols. */
const KEY_SYMBOLS: Record<string, string> = {
  space: 'Space',
  enter: '↩',
  tab: '⇥',
  escape: '⎋',
  esc: '⎋',
  backspace: '⌫',
  delete: '⌦',
  up: '↑',
  arrowup: '↑',
  down: '↓',
  arrowdown: '↓',
  left: '←',
  arrowleft: '←',
  right: '→',
  arrowright: '→',
  backquote: '`',
  minus: '-',
  equal: '=',
  bracketleft: '[',
  bracketright: ']',
  backslash: '\\',
  semicolon: ';',
  quote: "'",
  comma: ',',
  period: '.',
  slash: '/',
};

/** The keys Linux names in words where macOS draws a sign. */
const KEY_WORDS: Record<string, string> = { enter: 'Enter', tab: 'Tab', escape: 'Esc', esc: 'Esc', backspace: 'Backspace', delete: 'Delete' };

/** A shortcut as config.toml writes it (`Option+Cmd+/`) the way the platform shows it (⌥⌘/ on
 *  macOS, Alt+Super+/ on Linux), or null when it's off or unreadable. */
export function shortcutLabel(text: string | null | undefined): string | null {
  return shortcutKeys(text)?.join(platform() === 'mac' ? '' : '+') ?? null;
}

/** `shortcutLabel`'s keys one at a time (⌥ ⌘ /), for keycaps (primitives.tsx `Kbd`). */
export function shortcutKeys(text: string | null | undefined): string[] | null {
  const parts = (text ?? '')
    .split('+')
    .map((p) => p.trim())
    .filter(Boolean);
  if (parts.length === 0) return null;
  const mods = new Set<string>();
  let key: string | null = null;
  for (const p of parts) {
    const m = modOf(p);
    if (m) mods.add(m);
    else if (key === null) key = p;
    else return null;
  }
  if (!key) return null;
  const lower = key.toLowerCase();
  const mac = platform() === 'mac';
  const shown = (mac ? undefined : KEY_WORDS[lower]) ?? KEY_SYMBOLS[lower] ?? (/^key[a-z]$/.test(lower) ? lower.slice(3).toUpperCase() : /^digit\d$/.test(lower) ? lower.slice(5) : key.length === 1 ? key.toUpperCase() : key);
  return [...MODS.filter(([name]) => mods.has(name)).map(([, symbol, word]) => (mac ? symbol : word)), shown];
}

/** The modifiers a key event holds, as the platform shows them: ⌃⌥ on macOS, Ctrl+Alt on Linux.
 *  What Settings shows while it records a shortcut. */
export function heldSaid(e: { metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean }): string {
  const on = [e.ctrlKey, e.altKey, e.shiftKey, e.metaKey];
  const mac = platform() === 'mac';
  return MODS.filter((_, i) => on[i])
    .map(([, symbol, word]) => (mac ? symbol : word))
    .join(mac ? '' : '+');
}

/** Key codes (KeyboardEvent.code) as config.toml names them. */
function keyName(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1\d|20)$/.test(code)) return code;
  const named: Record<string, string> = {
    Space: 'Space',
    Enter: 'Enter',
    Tab: 'Tab',
    Backspace: 'Backspace',
    Delete: 'Delete',
    ArrowUp: 'Up',
    ArrowDown: 'Down',
    ArrowLeft: 'Left',
    ArrowRight: 'Right',
    Backquote: '`',
    Minus: '-',
    Equal: '=',
    BracketLeft: '[',
    BracketRight: ']',
    Backslash: '\\',
    Semicolon: ';',
    Quote: "'",
    Comma: ',',
    Period: '.',
    Slash: '/',
  };
  return named[code] ?? null;
}

/**
 * The shortcut a key press makes, as config.toml writes it, or why it can't be one. Keys are
 * read by where they sit (`code`), so ⌥ turning letters into symbols doesn't matter. The same
 * rules as the engine's: ⌃, ⌥ or ⌘ unless it's a function key, and not ⌘ alone with a key. On
 * Linux those are Ctrl, Alt and Super, and Ctrl alone is every app's; a page there never sees
 * Super (platform.ts), so a shortcut with it is written in config.toml.
 */
export function shortcutFromKey(e: { code: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean }): { shortcut: string } | { error: string } | null {
  if (['MetaLeft', 'MetaRight', 'OSLeft', 'OSRight', 'ControlLeft', 'ControlRight', 'AltLeft', 'AltRight', 'ShiftLeft', 'ShiftRight', 'CapsLock', 'Fn'].includes(e.code)) return null;
  const key = keyName(e.code);
  if (!key) return { error: 'That key can’t be part of a shortcut.' };
  const fn = /^F\d+$/.test(key);
  if (platform() === 'linux') {
    if (!fn && !e.ctrlKey && !e.altKey && !e.metaKey) return { error: 'Use Ctrl or Alt with it, or it would take a key you type.' };
    if (e.altKey && !e.ctrlKey && !e.metaKey && key === 'Tab') return { error: 'The desktop keeps Alt+Tab for switching windows.' };
    if (e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey && key !== 'Space' && !fn) return { error: `Ctrl+${shortcutLabel(key)} is a shortcut in most apps. Add Alt.` };
    const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Super'].filter(Boolean);
    return { shortcut: [...mods, key].join('+') };
  }
  if (!fn && !e.ctrlKey && !e.altKey && !e.metaKey) return { error: 'Use Control, Option or Command with it, or it would take a key you type.' };
  if (e.metaKey && !e.ctrlKey && !e.altKey && key === 'Tab') return { error: 'macOS keeps ⌘⇥ for switching apps.' };
  if (e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey && key !== 'Space' && !fn) return { error: `⌘${shortcutLabel(key)} is a menu shortcut in most apps. Add ⌃ or ⌥.` };
  const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Option', e.shiftKey && 'Shift', e.metaKey && 'Cmd'].filter(Boolean);
  return { shortcut: [...mods, key].join('+') };
}

/** Shortcuts macOS or common apps already use, so Settings can say what a new one would take
 *  over. Keyed by shortcutLabel's form. */
export const TAKEN: Record<string, string> = {
  '⌘Space': 'Spotlight',
  '⌥⌘Space': 'the Finder’s search window',
  '⌃Space': 'macOS’s input source switching, where it’s on',
  '⌃⌥Space': 'macOS’s input source switching, where it’s on',
  '⌃⌘Space': 'Emoji & Symbols',
  '⌥Space': 'the ChatGPT, Raycast and Alfred launchers, by default',
  '⇧⌘Space': '1Password’s Quick Access, by default',
  '⇧⌘3': 'screenshots',
  '⇧⌘4': 'screenshots',
  '⇧⌘5': 'screenshots',
  '⌃↑': 'Mission Control',
  '⌃↓': 'App Exposé',
  '⌃←': 'moving between Spaces',
  '⌃→': 'moving between Spaces',
};

/** The same on Linux, as GNOME and KDE have them by default. */
export const LINUX_TAKEN: Record<string, string> = {
  'Super+Space': 'GNOME’s input source switching',
  'Alt+Space': 'the window menu',
  'Alt+F2': 'the run dialog',
  'Alt+F4': 'closing windows',
  'Ctrl+Alt+T': 'a terminal, on Ubuntu',
  'Ctrl+Alt+L': 'locking the screen, on some desktops',
  'Ctrl+Alt+Delete': 'logging out',
  'Ctrl+Alt+↑': 'moving between workspaces',
  'Ctrl+Alt+↓': 'moving between workspaces',
  'Ctrl+Alt+←': 'moving between workspaces',
  'Ctrl+Alt+→': 'moving between workspaces',
  'Super+L': 'locking the screen',
  'Super+A': 'GNOME’s app grid',
  'Super+V': 'GNOME’s notifications',
};

/** What a shortcut (as `shortcutLabel` shows it) would take over on this platform. */
export function takenFor(label: string): string | undefined {
  return (platform() === 'mac' ? TAKEN : LINUX_TAKEN)[label];
}

/** How many past prompts ↑ can bring back. */
export const HISTORY_MAX = 50;

/** Past prompts with the newest first, after `text` is sent, started or saved: blank ones aren't
 *  kept, and one sent again moves to the front rather than showing twice. */
export function withPrompt(history: string[], text: string, max = HISTORY_MAX): string[] {
  if (!text.trim()) return history;
  return [text, ...history.filter((h) => h !== text)].slice(0, max);
}

/** What ↑ walks back through: the draft left when the panel last closed, then past prompts. */
export function recallList(draft: string | null, history: string[]): string[] {
  return draft?.trim() && draft !== history[0] ? [draft, ...history] : history;
}

/** What you'd typed in the panel without acting on it, and the todos it was for. */
export interface QuickDraft {
  text: string;
  todos: string[];
  /** Improve Codealong's pictures of the windows, when it was written over them (improve.ts). */
  shots?: Attachment[];
}

/** The draft the next open leaves: what's typed, or else the draft there already, since sending,
 *  starting or saving empties the input without making a draft of nothing. */
export function nextDraft(text: string, todos: string[], draft: QuickDraft | null, shots: Attachment[] = []): QuickDraft | null {
  return text.trim() ? { text, todos, ...(shots.length > 0 ? { shots } : {}) } : draft;
}

/** A draft as it's kept across restarts, as JSON; no draft takes the key out. */
export const draftCodec: Codec<QuickDraft | null> = {
  parse: (raw) => {
    const v: unknown = JSON.parse(raw);
    if (typeof v !== 'object' || v === null) return undefined;
    const { text, todos, shots } = v as { text?: unknown; todos?: unknown; shots?: unknown };
    if (typeof text !== 'string') return undefined;
    const kept = Array.isArray(shots) ? shots.filter(isShot) : [];
    return { text, todos: Array.isArray(todos) ? todos.filter((t): t is string => typeof t === 'string') : [], ...(kept.length > 0 ? { shots: kept } : {}) };
  },
  format: (draft) => (draft?.text.trim() ? JSON.stringify(draft) : null),
};

function isShot(v: unknown): v is Attachment {
  if (typeof v !== 'object' || v === null) return false;
  const { kind, path, name } = v as { kind?: unknown; path?: unknown; name?: unknown };
  return kind === 'image' && typeof path === 'string' && path.startsWith('images/') && typeof name === 'string';
}

/** The start row's title: where it starts, as Jev's start suggestions say theirs (start.ts
 *  `startTitleWords`, null before there's a worktree), and the todos it's for. */
export function startTitle(todos: number, where: string | null): string {
  const title = where ?? 'Start a new session';
  if (todos > 1) return `${title} for these ${todos} todos`;
  return todos === 1 ? `${title} for this todo` : title;
}

/** What the start row says first when a todo it's for has a primary session that's running
 *  (todos.ts `primaryHolds`): the new session would work alongside it, not in its place. */
export function primaryRunningWords(held: { todo: Pick<Todo, 'title'>; at: { lane: Pick<Lane, 'callsign'>; session: Pick<Session, 'title' | 'account'> } }[], todos: number): string {
  const [first] = held;
  if (!first) return '';
  const who = `“${first.at.session.title}” (${first.at.session.account} · ${first.at.lane.callsign})`;
  if (held.length > 1) return `${who} and ${held.length - 1} more are running as these todos’ primary sessions: this one would work alongside them. `;
  const whose = todos > 1 ? `“${first.todo.title || 'Untitled'}”’s` : 'this todo’s';
  return `${who} is ${whose} primary session and is running: this one would work alongside it. `;
}

/** What the start row adds about the files attached to the prompt. */
export function attachmentsNote(count: number): string {
  return ` ${count === 1 ? 'The attachment goes' : 'Attachments go'} as ${count === 1 ? 'a file path' : 'file paths'} the agent opens.`;
}

/** What the start row adds when a session for todos starts on another Mac: the todos are this
 *  Mac's, so the session isn't linked to them. */
export function todosStayNote(count: number): string {
  return ` Todos are this Mac’s, so ${count > 1 ? 'they aren’t' : 'it isn’t'} linked to the session.`;
}

/** A Mac's worktrees in the project, for its line in the Mac menu. */
export function worktreeCount(count: number, project: string): string {
  return `${count} worktree${count === 1 ? '' : 's'}${project === ALL_PROJECTS ? '' : ` in ${project}`}`;
}

/** Why another Mac can't start a session now, for its line in the Mac menu: it's away, or it
 *  doesn't take remote control (with what to do about that). */
export function macUnavailable(d: Pick<DeviceView, 'name' | 'online' | 'lastSeen' | 'remoteControl'>, now: number): { away: string | null; off: string | null; title: string | undefined } {
  const away = !d.online ? (d.lastSeen ? `Away: last seen ${agoPhrase(d.lastSeen, now)}` : 'Away') : null;
  const off = d.online && !d.remoteControl ? 'Remote control is off there' : null;
  return { away, off, title: off ? `Remote control is off on ${d.name}; turn it on there in Settings › Devices to start sessions from here.` : undefined };
}
