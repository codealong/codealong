// What the quick panel lists, and how it names shortcuts. With nothing typed it lists the
// sessions that need you, then the ones whose turn ended; with nothing waiting, the ones
// working. Under them go the first few todos ready to start, then the rest of the sessions
// started in the last day, which ↓ reaches past the rest. Typed words find sessions, then
// todos, then PRs: every word has to be in what describes them, or, for a session, in what was
// said in it (the engine's text search, search.rs). Kept free of React so the rules have tests.

import { agoPhrase, middleTruncate } from './format';
import { heldForAgent } from './prStatus';
import { branchLabel, byStatusTime, sessionHomes, unattended } from './lanes';
import { inQueueOrder, reviewWhere, sessionWhere, statusWaits, type Reads } from './queue';
import { sectionOf, todoItem, type TodoItem, type TodoSectionId } from './todos';
import { prFlag } from './prStatus';
import type { Board, DeviceView, Lane, PrItem, PrState, Session, Status, TextHit, Todo } from './types';

export const ALL_PROJECTS = 'All projects';
/** Codealong's chats, as the worktree picker offers them: a chat of its own, in no project. */
export const NO_PROJECT = 'No project';
const ELSEWHERE = 'Elsewhere';

/** Whether a worktree (or a floating bar pill, which carries its worktree's project and whether
 *  it's hidden) is in the project the panel shows. Elsewhere's worktrees stay out of All
 *  projects, as they do in the main window. */
export function inProject(lane: Pick<Lane, 'project' | 'hidden'>, project: string): boolean {
  if (project === ALL_PROJECTS) return !lane.hidden;
  return lane.project === project && (project === ELSEWHERE || !lane.hidden);
}

export type RowGroup = 'suggested' | 'skills' | 'needs' | 'reviews' | 'unread' | 'working' | 'found' | 'todos' | 'prs' | 'ready' | 'recent';

/** Where a status puts a session in the empty panel's list, as on the floating bar: errors wait
 *  on you like questions do. */
function groupOf(status: Status): 'needs' | 'unread' | 'working' | null {
  if (status === 'needs' || status === 'error') return 'needs';
  if (status === 'unread' || status === 'working') return status;
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
  /** Found by what was said rather than what describes it: a line of that, and where the
   *  words are in it. */
  said?: { text: string; you: boolean; marks: [number, number][] };
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
  statusOf: (r) => (r.kind === 'session' ? r.session.status : null),
  whereOf: (r) => (r.kind === 'session' ? sessionWhere(r.lane.callsign, r.session.account) : reviewWhere(r.pr.repo, r.pr.requestedBy)),
  emblemOf: (r) => (r.kind === 'session' ? r.session.emblem : undefined),
};

/** The empty panel's list: what waits on you in the order the waiting window works through it
 *  (queue.ts `byPlace`), sessions that need you, then the PRs waiting on your review, then the
 *  sessions whose turn ended, each the longest wait first, as on the floating bar and in the
 *  notch (bar.ts `NOTCH_ORDER`); with none of those, the ones working, the longest running first.
 *  `counts` has every group's full size, for the headings. */
export function attentionRows(board: Board, project: string, limit = MAX_ROWS): { rows: QueueRow[]; counts: Record<'needs' | 'reviews' | 'unread' | 'working', number> } {
  // A helper or a headless run waits here only while it needs you, as on the floating bar
  // (barfeed.rs).
  const sessions = sessionsIn(board, project)
    .flatMap(({ lane, session }) => {
      const group = unattended(session) ? null : groupOf(session.status);
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
 * Where `word` first appears in `text` (lowercased), or -1. Among other words, a word of one or
 * two letters counts only whole: "class c" shouldn't find every session with a c in it (every
 * `claude` account has one). Alone it's a search as you type and matches anywhere. The engine's
 * text search has the same rule (`search::has_word`).
 */
export function wordAt(text: string, word: string, of: number): number {
  if (of < 2 || word.length > 2) return text.indexOf(word);
  const edge = (c: string | undefined) => !c || !/[\p{L}\p{N}]/u.test(c);
  for (let i = text.indexOf(word); i >= 0; i = text.indexOf(word, i + 1)) {
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

const STATUS_RANK: Record<Status, number> = { needs: 0, error: 0, unread: 1, working: 2, idle: 3, stopped: 4 };

/** The shortest search that's sent to the text search: fewer letters match nearly everything. */
export const TEXT_SEARCH_MIN = 3;

/**
 * Sessions with every typed word somewhere in what describes them, or in what was said in them
 * (`said`, the text search's hits by session key), best first: those whose title has them all,
 * then the rest of what describes them, then what was said, the best match first there (the
 * words as typed beat them apart); within each by status (needs you, your turn, working, the
 * rest), then the most recently active. Archived sessions are left out, unless `archived` asks
 * for them: then they come after all the others, in the same order among themselves.
 */
export function findSessions(board: Board, project: string, query: string, limit = MAX_ROWS, said: Map<string, TextHit> = new Map(), archived: Archived = 'without'): SessionRow[] {
  const words = queryWords(query);
  if (words.length === 0) return [];
  const found = sessionsIn(board, project, archived).flatMap(({ lane, session: s }) => {
    const pr = lane.pr ? `#${lane.pr.number} ${lane.pr.title}` : '';
    const hay = [s.title, s.account, lane.callsign, lane.repo, lane.branch ?? '', s.branch ?? '', lane.project, pr].join('\n').toLowerCase();
    const title = s.title.toLowerCase();
    const row: SessionRow = { kind: 'session', key: s.key, group: 'found', lane, session: s, marks: titleMarks(s.title, words) };
    if (hasWords(title, words)) return [{ row, tier: 0, score: 0 }];
    if (hasWords(hay, words)) return [{ row, tier: 1, score: 0 }];
    const hit = said.get(s.key);
    if (!hit) return [];
    return [{ row: { ...row, said: { text: hit.snippet, you: hit.you, marks: titleMarks(hit.snippet, words) } }, tier: 2, score: hit.score ?? 0 }];
  });
  return found
    .sort(
      (a, b) =>
        Number(a.row.session.archived) - Number(b.row.session.archived) ||
        a.tier - b.tier ||
        b.score - a.score ||
        STATUS_RANK[a.row.session.status] - STATUS_RANK[b.row.session.status] ||
        b.row.session.lastActivity - a.row.session.lastActivity ||
        a.row.key.localeCompare(b.row.key),
    )
    .slice(0, limit)
    .map((f) => f.row);
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

const SECTION_RANK: Record<TodoSectionId, number> = { 'in-progress': 0, ready: 1, waiting: 2, done: 3 };

/**
 * Todos with every typed word in their title, description, labels or project, or in the
 * worktrees and sessions linked to them, best first: those whose title has them all, then In
 * progress, Ready, Waiting and Done, then the latest activity. In a project, only its own todos.
 */
export function findTodos(board: Board, project: string, query: string, limit = FOUND_TODOS): TodoRow[] {
  const words = queryWords(query);
  if (words.length === 0) return [];
  const todos = board.todos ?? [];
  const laneById = new Map(board.lanes.map((l) => [l.id, l]));
  const homes = sessionHomes(board.lanes);
  const labelById = new Map((board.labels ?? []).map((l) => [l.id, l]));
  const found = todos
    .filter((t) => project === ALL_PROJECTS || t.project === project)
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
      const section = sectionOf(t, todos);
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

/** Where a new session can start, for the worktree menu: pinned worktrees first, in your order,
 *  then the rest by repo, the most recently active repo first. In each repo its main checkout
 *  leads, then the most recently active worktrees: the main checkout can be on any branch, so
 *  its place is what finds it. */
export function laneGroups(lanes: Lane[], pinned: string[]): { label: string; lanes: Lane[] }[] {
  const byId = new Map(lanes.map((l) => [l.id, l]));
  const pins = pinned.flatMap((id) => byId.get(id) ?? []);
  const pinnedIds = new Set(pins.map((l) => l.id));
  const projects = new Set(lanes.map((l) => l.project)).size;
  const groups = new Map<string, Lane[]>();
  for (const l of lanes) {
    if (pinnedIds.has(l.id)) continue;
    const label = projects > 1 ? `${l.project} · ${l.repo}` : l.repo;
    groups.set(label, [...(groups.get(label) ?? []), l]);
  }
  const latest = (ls: Lane[]) => Math.max(...ls.map((l) => l.lastActivity));
  const repos = [...groups.entries()]
    .map(([label, ls]) => ({ label, lanes: [...ls].sort((a, b) => Number(b.origin === 'main') - Number(a.origin === 'main') || b.lastActivity - a.lastActivity) }))
    .sort((a, b) => latest(b.lanes) - latest(a.lanes));
  return pins.length > 0 ? [{ label: 'Pinned', lanes: pins }, ...repos] : repos;
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
      const g = groupOf(session.status);
      if (g === 'needs' || g === 'unread') c[g] += 1;
    }
    out[name] = c;
  }
  return out;
}

/* ---- Shortcuts ---- */

/** Modifiers in the order macOS writes them. */
const MODS: [string, string, string][] = [
  ['Ctrl', '⌃', 'Control'],
  ['Option', '⌥', 'Option'],
  ['Shift', '⇧', 'Shift'],
  ['Cmd', '⌘', 'Command'],
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

/** A shortcut as config.toml writes it (`Option+Cmd+/`) the way macOS shows it (⌥⌘/),
 *  or null when it's off or unreadable. */
export function shortcutLabel(text: string | null | undefined): string | null {
  const parts = (text ?? '')
    .split('+')
    .map((p) => p.trim())
    .filter(Boolean);
  if (parts.length === 0) return null;
  const mods = new Set<string>();
  let key: string | null = null;
  for (const p of parts) {
    const m = MOD_ALIASES[p.toLowerCase()];
    if (m) mods.add(m);
    else if (key === null) key = p;
    else return null;
  }
  if (!key) return null;
  const lower = key.toLowerCase();
  const shown = KEY_SYMBOLS[lower] ?? (/^key[a-z]$/.test(lower) ? lower.slice(3).toUpperCase() : /^digit\d$/.test(lower) ? lower.slice(5) : key.length === 1 ? key.toUpperCase() : key);
  return `${MODS.filter(([name]) => mods.has(name))
    .map(([, symbol]) => symbol)
    .join('')}${shown}`;
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
 * rules as the engine's: ⌃, ⌥ or ⌘ unless it's a function key, and not ⌘ alone with a key.
 */
export function shortcutFromKey(e: { code: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean }): { shortcut: string } | { error: string } | null {
  if (['MetaLeft', 'MetaRight', 'ControlLeft', 'ControlRight', 'AltLeft', 'AltRight', 'ShiftLeft', 'ShiftRight', 'CapsLock', 'Fn'].includes(e.code)) return null;
  const key = keyName(e.code);
  if (!key) return { error: 'That key can’t be part of a shortcut.' };
  const fn = /^F\d+$/.test(key);
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
