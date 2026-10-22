// Boards: named sets of work in a project, each shown as Kanban or as Canvas. A board holds what
// its rule takes in (rules.ts) and what you added by hand, less what you took off; its Kanban
// columns, where you put cards by hand and in what order; and its canvas (canvas/model.ts). A
// project has only the boards made in it: none until one's made, when the Board view says what a
// board is and offers to make one. The engine keeps them in boards.json (boards.rs) and makes
// every change; a change is made here too as it's asked (`applyOp`, which does what boards.rs
// `apply_to` does), so the window shows it at once.

import type { OnBoard, RefKind } from '../canvas/model';
import { ALL_PROJECTS } from '../quick';
import type { Label } from '../types';

/** How far a card's work has got, as the stage columns have it (rules.ts `stageOf`). */
export type Stage = 'todo' | 'progress' | 'review' | 'done';
/** An agent's status, as the sidebar's statuses name them. */
export type StatusPick = 'needs' | 'unread' | 'working';
/** How recently a card's work moved on. */
export type Active = '1d' | '3d' | '7d' | '30d';

export const STAGES: Stage[] = ['todo', 'progress', 'review', 'done'];
export const STAGE_WORD: Record<Stage, string> = { todo: 'To do', progress: 'In progress', review: 'In review', done: 'Done' };
export const STATUS_PICKS: StatusPick[] = ['needs', 'unread', 'working'];
export const STATUS_WORD: Record<StatusPick, string> = { needs: 'Needs you', unread: 'Your turn', working: 'Working' };
export const ACTIVES: Active[] = ['1d', '3d', '7d', '30d'];
export const KINDS: RefKind[] = ['todo', 'session', 'worktree', 'pr'];
export const KIND_WORD: Record<RefKind, string> = { todo: 'Todos', session: 'Sessions', worktree: 'Worktrees', pr: 'PRs' };
/** One of a kind, as a count says it: `3 sessions`. */
export const ONE_WORD: Record<RefKind, string> = { todo: 'todo', session: 'session', worktree: 'worktree', pr: 'PR' };

/**
 * What a rule takes in (rules.ts `matches`). Each part it has must hold, and a part holds when a
 * card has any of what it lists; a rule with no parts takes in everything.
 */
export interface Rule {
  kinds?: RefKind[];
  stages?: Stage[];
  statuses?: StatusPick[];
  /** Todo labels by id: todos that have one, and the sessions and worktrees on them. */
  labels?: string[];
  /** Repos by GitHub slug. */
  repos?: string[];
  accounts?: string[];
  active?: Active;
  /** Words its title has. */
  text?: string;
}

/** How a column orders its cards: as you put them, by whose move it is, or the latest first. */
export type Sort = 'hand' | 'move' | 'latest';

/** A Kanban column. A card goes in the first column whose rule takes it, else in the column you
 *  put it in, else in the first column without a rule (rules.ts `columnOf`). */
export interface BoardColumn {
  id: string;
  name: string;
  rule?: Rule | null;
  sort: Sort;
  /** Folded to a rail until it's opened. */
  folded?: boolean;
}

export interface BoardDoc {
  id: string;
  name: string;
  project: string;
  /** What comes onto it by itself; null for only what's added by hand. */
  rule: Rule | null;
  /** Cards added by hand and taken off by hand, by card id (`refId`). */
  added: string[];
  removed: string[];
  /** When each card was added by hand. */
  addedAt?: Record<string, number>;
  /** Its Kanban columns; null for the four stages. */
  columns?: BoardColumn[] | null;
  /** The column each card was put in by hand, by card id. */
  placed?: Record<string, string>;
  /** Cards in the order you put them. */
  order?: string[];
  createdAt?: number;
}

/** A change to a board, as boards.rs `BoardOp` takes it. */
export type BoardOp =
  | { op: 'create'; board: BoardDoc }
  | { op: 'rename'; id: string; name: string }
  | { op: 'project'; id: string; project: string }
  | { op: 'delete'; id: string }
  | { op: 'add'; id: string; cards: string[] }
  | { op: 'remove'; id: string; cards: string[] }
  | { op: 'rule'; id: string; rule: Rule | null }
  | { op: 'columns'; id: string; columns: BoardColumn[] | null }
  | { op: 'place'; id: string; card: string; column: string | null }
  | { op: 'order'; id: string; order: string[] };

/** The stage columns: they follow the work, and a card dropped on one moves its work on (kanban.ts). */
export const STAGE_COLUMNS: BoardColumn[] = [
  { id: 'todo', name: 'To do', rule: { stages: ['todo'] }, sort: 'hand' },
  { id: 'progress', name: 'In progress', rule: { stages: ['progress'] }, sort: 'move' },
  { id: 'review', name: 'In review', rule: { stages: ['review'] }, sort: 'move' },
  { id: 'done', name: 'Done', rule: { stages: ['done'] }, sort: 'latest', folded: true },
];

export function columnsOf(b: BoardDoc): BoardColumn[] {
  return b.columns ?? STAGE_COLUMNS;
}

/** The one stage a column's rule is about, when it's a stage column: its glyph and its words. */
export function stageOfColumn(c: BoardColumn): Stage | null {
  const r = c.rule;
  if (!r || r.stages?.length !== 1) return null;
  const others = Object.entries(r).filter(([k, v]) => k !== 'stages' && v != null && (!Array.isArray(v) || v.length > 0));
  return others.length === 0 ? r.stages[0] : null;
}

export type ColumnsPreset = 'stages' | 'labels' | 'blank';

/** The columns a new board starts with (cli.rs `ColumnsPreset` makes the same): the four stages
 *  (null), one column per label and Other, or one column, Inbox. */
export function presetColumns(preset: ColumnsPreset, labels: Label[]): BoardColumn[] | null {
  switch (preset) {
    case 'stages':
      return null;
    case 'blank':
      return [{ id: 'inbox', name: 'Inbox', sort: 'hand' }];
    case 'labels':
      return [...labels.map((l): BoardColumn => ({ id: `label-${l.id}`, name: l.name, rule: { labels: [l.id] }, sort: 'move' })), { id: 'other', name: 'Other', sort: 'hand' }];
  }
}

export function newBoardId(): string {
  return `b-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

export function newColumnId(taken: BoardColumn[]): string {
  for (let n = taken.length + 1; ; n++) {
    const id = `c${n}`;
    if (!taken.some((c) => c.id === id)) return id;
  }
}

/** A rule with nothing in it: everything. */
export function takesAll(r: Rule | null): boolean {
  return !!r && Object.values(r).every((v) => v == null || (Array.isArray(v) && v.length === 0) || v === '');
}

/** A rule without its empty parts, as it's saved. */
export function cleanRule(r: Rule): Rule {
  const out: Rule = {};
  for (const [k, v] of Object.entries(r) as [keyof Rule, Rule[keyof Rule]][]) {
    if (v == null || v === '' || (Array.isArray(v) && v.length === 0)) continue;
    (out as Record<string, unknown>)[k] = typeof v === 'string' ? v.trim() : v;
  }
  return out;
}

/** What a rule takes in, in words (cli.rs `rule_text` says it the same way). */
export function ruleWords(r: Rule | null, labels: Label[]): string {
  if (!r) return 'Only what’s added';
  const or = (xs: string[]) => xs.join(' or ');
  const parts: string[] = [];
  if (r.kinds?.length) parts.push(or(r.kinds.map((k) => KIND_WORD[k].toLowerCase().replace('prs', 'PRs'))));
  if (r.labels?.length) parts.push(`labelled ${or(r.labels.map((id) => labels.find((l) => l.id === id)?.name ?? 'a deleted label'))}`);
  if (r.stages?.length) parts.push(or(r.stages.map((s) => STAGE_WORD[s].toLowerCase())));
  if (r.statuses?.length) parts.push(or(r.statuses.map((s) => STATUS_WORD[s].toLowerCase())));
  if (r.repos?.length) parts.push(`in ${or(r.repos)}`);
  if (r.accounts?.length) parts.push(`on ${or(r.accounts)}`);
  if (r.active) parts.push(`active in the last ${r.active}`);
  if (r.text?.trim()) parts.push(`titled “${r.text.trim()}”`);
  if (parts.length === 0) return 'Everything';
  const words = parts.join(', ');
  return words.charAt(0).toUpperCase() + words.slice(1);
}

const cleanName = (name: string) => name.split(/\s+/).filter(Boolean).join(' ').slice(0, 100);
const without = (list: string[] | undefined, gone: string[]) => (list ?? []).filter((c) => !gone.includes(c));

/**
 * The boards with `op` made, as boards.rs `apply_to` makes it, or the same list when it can't be
 * made here (the engine then says why).
 */
export function applyOp(boards: BoardDoc[], op: BoardOp, now: number): BoardDoc[] {
  if (op.op === 'create') {
    // What a board is made with was added as it was made.
    const addedAt = { ...Object.fromEntries(op.board.added.map((c) => [c, now])), ...op.board.addedAt };
    return [...boards, { ...op.board, name: cleanName(op.board.name), addedAt, createdAt: now }];
  }
  if (op.op === 'delete') return boards.filter((b) => b.id !== op.id);
  const at = boards.findIndex((b) => b.id === op.id);
  if (at < 0) return boards;
  const b = boards[at];
  let next: BoardDoc;
  switch (op.op) {
    case 'rename':
      next = { ...b, name: cleanName(op.name) || b.name };
      break;
    case 'project':
      next = { ...b, project: op.project.trim() || b.project };
      break;
    case 'add': {
      const fresh = op.cards.filter((c) => !b.added.includes(c));
      next = { ...b, removed: without(b.removed, op.cards), added: [...b.added, ...fresh], addedAt: { ...b.addedAt, ...Object.fromEntries(fresh.map((c) => [c, now])) } };
      break;
    }
    case 'remove': {
      const kept = <T,>(m: Record<string, T> | undefined) => Object.fromEntries(Object.entries(m ?? {}).filter(([c]) => !op.cards.includes(c)));
      next = { ...b, added: without(b.added, op.cards), removed: [...b.removed, ...op.cards.filter((c) => !b.removed.includes(c))], placed: kept(b.placed), addedAt: kept(b.addedAt), order: without(b.order, op.cards) };
      break;
    }
    case 'rule':
      next = { ...b, rule: op.rule };
      break;
    case 'columns': {
      const ids = new Set((op.columns ?? []).map((c) => c.id));
      next = { ...b, columns: op.columns, placed: Object.fromEntries(Object.entries(b.placed ?? {}).filter(([, col]) => ids.has(col))) };
      break;
    }
    case 'place': {
      const placed = { ...(b.placed ?? {}) };
      if (op.column) placed[op.card] = op.column;
      else delete placed[op.card];
      next = { ...b, placed };
      break;
    }
    case 'order':
      next = { ...b, order: [...new Set(op.order)] };
      break;
  }
  return boards.map((x, i) => (i === at ? next : x));
}

/** What a board shows on its canvas (canvas/model.ts `OnBoard`): `shows` from boardLists.ts
 *  `canvasCards`, `member` from boards/rules.ts `memberTest`. */
export function holdsOf(b: BoardDoc, member: (card: string) => boolean, shows: ReadonlySet<string>): OnBoard {
  return { shows, has: member, removed: new Set(b.removed) };
}

/** A board as the engine sends it, with what a window counts on filled in. */
export function readBoard(raw: BoardDoc): BoardDoc {
  return { ...raw, rule: raw.rule ?? null, added: raw.added ?? [], removed: raw.removed ?? [] };
}

/** Where a window is, as far as following a board it moved goes. */
export interface BoardPlace {
  project: string;
  /** Whether it shows the Board view (Kanban or Canvas). */
  showing: boolean;
  board: string;
}

/** Whether a window follows a board it moved to its new project, once the move is made: only when
 *  it was showing that board in a project's list (All projects lists it still), and it's where it
 *  was when it asked, so a move that comes back late doesn't take it from where it's gone since. */
export function followsMove(board: string, asked: BoardPlace, now: BoardPlace): boolean {
  const same = asked.project === now.project && asked.showing === now.showing && asked.board === now.board;
  return asked.showing && asked.board === board && asked.project !== ALL_PROJECTS && same;
}

/** A board's name as a menu lists it: with its project when the window shows another (All projects). */
export function boardName(b: BoardDoc, shown: string): string {
  return b.project === shown ? b.name : `${b.name} · ${b.project}`;
}

/** The boards that show a thing of `project` once it's put on them: every project's board and its
 *  own project's (a board shows only its project's work); every one, for a thing whose project
 *  isn't known. */
export function boardsFor(boards: BoardDoc[], project: string | null): BoardDoc[] {
  return project == null ? boards : boards.filter((b) => b.project === ALL_PROJECTS || b.project === project);
}

/** Add to board ›'s choices for things picked together, which may be of several projects: each
 *  board that would show any of them (`boardsFor`), with the ones it would show, those of them it
 *  doesn't hold yet, and whether it holds every one (`holds` asks of each board once: it can read
 *  everything the window has). A pick puts on the ones it doesn't hold, or takes them all off when
 *  it holds them all, as Pin does for several. */
export function boardChoices(boards: BoardDoc[], cards: { card: string; project: string | null }[], holds: (b: BoardDoc) => (card: string) => boolean): { board: BoardDoc; cards: string[]; fresh: string[]; on: boolean }[] {
  return boards.flatMap((board) => {
    const shown = cards.filter((c) => boardsFor([board], c.project).length > 0).map((c) => c.card);
    if (shown.length === 0) return [];
    const has = holds(board);
    const fresh = shown.filter((c) => !has(c));
    return [{ board, cards: shown, fresh, on: fresh.length === 0 }];
  });
}

/** A project's boards as they're listed, as they were made. All projects lists every project's
 *  after its own, as the engine sends them (boards.rs `listed`): a board shows its own project's
 *  work wherever it's shown. */
export function boardsIn(boards: BoardDoc[], project: string): BoardDoc[] {
  const every = project === ALL_PROJECTS;
  const order = (a: BoardDoc, b: BoardDoc) => Number(a.project !== project) - Number(b.project !== project) || (a.project < b.project ? -1 : a.project > b.project ? 1 : 0) || (a.createdAt ?? 0) - (b.createdAt ?? 0);
  return boards.filter((b) => every || b.project === project).sort(order);
}
