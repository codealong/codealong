// What the quick panel lists, and how it names shortcuts. With nothing typed it lists the
// sessions that need you, then the ones whose turn ended; with nothing waiting, the ones
// working. Typed words find sessions, then todos: every word has to be in what describes them.
// Kept free of React so the rules have tests.

import { middleTruncate } from './format';
import { sectionOf, sessionHomes, todoItem, type TodoItem, type TodoSectionId } from './todos';
import type { Board, Lane, Session, Status } from './types';

export const ALL_PROJECTS = 'All projects';
const ELSEWHERE = 'Elsewhere';

/** Whether a worktree is in the project the panel shows. Elsewhere's worktrees stay out of All
 *  projects, as they do in the main window. */
export function inProject(lane: Lane, project: string): boolean {
  if (project === ALL_PROJECTS) return !lane.hidden;
  return lane.project === project && (project === ELSEWHERE || !lane.hidden);
}

export type RowGroup = 'needs' | 'unread' | 'working' | 'found' | 'todos';

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
}

/** How many rows the panel lists at most: ⌘1 to ⌘9 reach them all. */
export const MAX_ROWS = 9;
/** A search lists this many sessions and this many todos at most, so both fit. */
export const FOUND_SESSIONS = 6;
export const FOUND_TODOS = 5;

/** Every session once, listed under the worktree it runs in (not as a visitor elsewhere), with
 *  archived ones left out. */
function sessionsIn(board: Board, project: string): { lane: Lane; session: Session }[] {
  const out: { lane: Lane; session: Session }[] = [];
  for (const lane of board.lanes) {
    if (!inProject(lane, project)) continue;
    for (const session of lane.sessions) if (!session.visitor && !session.archived) out.push({ lane, session });
  }
  return out;
}

const since = (s: Session) => s.statusSince ?? s.lastActivity;

/** The empty panel's list: sessions that need you, then those whose turn ended, the newest
 *  status first, as on the floating bar; with neither, the ones working. `counts` has every
 *  group's full size, for the headings. */
export function attentionRows(board: Board, project: string, limit = MAX_ROWS): { rows: SessionRow[]; counts: Record<'needs' | 'unread' | 'working', number> } {
  const all = sessionsIn(board, project)
    .flatMap(({ lane, session }) => {
      const group = groupOf(session.status);
      return group ? [{ kind: 'session' as const, key: session.key, group, lane, session, marks: [] }] : [];
    })
    .sort((a, b) => ORDER.indexOf(a.group) - ORDER.indexOf(b.group) || since(b.session) - since(a.session) || a.key.localeCompare(b.key));
  const counts = { needs: 0, unread: 0, working: 0 };
  for (const r of all) counts[r.group as keyof typeof counts] += 1;
  const waiting = all.filter((r) => r.group !== 'working');
  return { rows: (waiting.length > 0 ? waiting : all).slice(0, limit), counts };
}

const ORDER: RowGroup[] = ['needs', 'unread', 'working'];

/** The words to look for, lowercased. */
export function queryWords(query: string): string[] {
  return query.toLowerCase().split(/\s+/).filter(Boolean);
}

/** Where each word first appears in the title, merged where they overlap. */
export function titleMarks(title: string, words: string[]): [number, number][] {
  const t = title.toLowerCase();
  const spans = words
    .map((w) => [t.indexOf(w), w.length] as const)
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

/**
 * Sessions with every typed word somewhere in what describes them, best first: those whose
 * title has them all, then by status (needs you, your turn, working, the rest), then the most
 * recently active. Archived sessions are left out.
 */
export function findSessions(board: Board, project: string, query: string, limit = MAX_ROWS): SessionRow[] {
  const words = queryWords(query);
  if (words.length === 0) return [];
  const found = sessionsIn(board, project).flatMap(({ lane, session: s }) => {
    const pr = lane.pr ? `#${lane.pr.number} ${lane.pr.title}` : '';
    const hay = [s.title, s.account, lane.callsign, lane.repo, lane.branch ?? '', s.branch ?? '', lane.project, pr].join('\n').toLowerCase();
    if (!words.every((w) => hay.includes(w))) return [];
    const title = s.title.toLowerCase();
    return [{ row: { kind: 'session' as const, key: s.key, group: 'found' as const, lane, session: s, marks: titleMarks(s.title, words) }, inTitle: words.every((w) => title.includes(w)) }];
  });
  return found
    .sort(
      (a, b) =>
        Number(b.inTitle) - Number(a.inTitle) ||
        STATUS_RANK[a.row.session.status] - STATUS_RANK[b.row.session.status] ||
        b.row.session.lastActivity - a.row.session.lastActivity ||
        a.row.key.localeCompare(b.row.key),
    )
    .slice(0, limit)
    .map((f) => f.row);
}

/** A todo a search found. */
export interface TodoRow {
  kind: 'todo';
  key: string;
  group: 'todos';
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
      if (!words.every((w) => hay.includes(w))) return [];
      const section = sectionOf(t, todos);
      const title = t.title.toLowerCase();
      const row: TodoRow = { kind: 'todo', key: `todo:${t.id}`, group: 'todos', item, waiting: section === 'waiting', marks: titleMarks(t.title, words) };
      return [{ row, inTitle: words.every((w) => title.includes(w)), rank: SECTION_RANK[section] }];
    });
  return found
    .sort((a, b) => Number(b.inTitle) - Number(a.inTitle) || a.rank - b.rank || b.row.item.lastActivity - a.row.item.lastActivity || a.row.key.localeCompare(b.row.key))
    .slice(0, limit)
    .map((f) => f.row);
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
  const branch = l.branch ?? (l.detached ? `detached @ ${l.detached}` : null);
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

/** A shortcut as config.toml writes it (`Option+Shift+Space`) the way macOS shows it (⌥⇧Space),
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
