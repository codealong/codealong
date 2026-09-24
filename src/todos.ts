// Todos: work you mean to hand an agent. The engine keeps them in todos.json
// (src-tauri/src/todos.rs) and publishes them with the board. This file has the list's rules:
// its sections, search and the Inbox filters, the worktrees and sessions each todo is linked
// to, the prompt a todo starts a session with, and the copy of each change a window shows
// until the engine's board has it.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { todoOp } from './api';
import type { Board, Label, LabelColor, Lane, Pr, Session, Todo, TodoStatus } from './types';
import { LABEL_COLORS } from './types';

export type TodoPatch = Partial<Pick<Todo, 'title' | 'description' | 'status' | 'labels' | 'project'>>;

/** A change, as the engine takes it (`TodoOp` in todos.rs). */
export type TodoOp =
  | { op: 'create'; todo: Todo }
  | { op: 'update'; id: string; patch: TodoPatch }
  | { op: 'link'; id: string; worktree?: string; session?: string; linked: boolean }
  | { op: 'delete'; id: string }
  | { op: 'restore'; todo: Todo; index: number }
  | { op: 'order'; ids: string[] }
  | { op: 'labels'; labels: Label[] };

export interface TodoStore {
  todos: Todo[];
  labels: Label[];
}

export const STATUS_WORD: Record<TodoStatus, string> = { todo: 'Todo', 'in-progress': 'In progress', done: 'Done' };
export const STATUSES: TodoStatus[] = ['todo', 'in-progress', 'done'];
export const COLOR_WORD: Record<LabelColor, string> = {
  gray: 'Gray',
  red: 'Red',
  pink: 'Pink',
  purple: 'Purple',
  indigo: 'Indigo',
  blue: 'Blue',
  teal: 'Teal',
  green: 'Green',
};

/** An id the engine accepts: a prefix, the time and some randomness. */
export function newId(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
}

export function newTodo(fields: Partial<Todo>, now: number): Todo {
  return {
    id: newId('t'),
    project: '',
    title: '',
    description: '',
    status: 'todo',
    labels: [],
    worktrees: [],
    sessions: [],
    createdAt: now,
    updatedAt: now,
    doneAt: null,
    pending: [],
    ...fields,
  };
}

/** The color a new label gets: the first one no label has yet, else the next in turn. */
export function nextColor(labels: Label[]): LabelColor {
  const free = LABEL_COLORS.find((c) => c !== 'gray' && !labels.some((l) => l.color === c));
  return free ?? LABEL_COLORS[labels.length % LABEL_COLORS.length];
}

function withStatus(t: Todo, status: TodoStatus, now: number): Todo {
  if (t.status === status) return t;
  return { ...t, status, doneAt: status === 'done' ? now : null };
}

function uniq(list: string[]): string[] {
  return [...new Set(list.filter(Boolean))];
}

/** A change applied the way the engine applies it, so a window can show it at once. */
export function applyOp(store: TodoStore, op: TodoOp, now: number): TodoStore {
  const map = (id: string, f: (t: Todo) => Todo) => ({ ...store, todos: store.todos.map((t) => (t.id === id ? f(t) : t)) });
  switch (op.op) {
    case 'create': {
      if (store.todos.some((t) => t.id === op.todo.id)) return store;
      let t: Todo = { ...op.todo, worktrees: uniq(op.todo.worktrees), sessions: uniq(op.todo.sessions), pending: [] };
      if (t.sessions.length > 0 && t.status === 'todo') t = withStatus(t, 'in-progress', now);
      return { ...store, todos: [t, ...store.todos] };
    }
    case 'update':
      return map(op.id, (t) => {
        const { status, ...rest } = op.patch;
        const next = { ...t, ...rest, updatedAt: now };
        return status ? withStatus(next, status, now) : next;
      });
    case 'link':
      return map(op.id, (t) => {
        if (!op.linked) {
          return {
            ...t,
            worktrees: t.worktrees.filter((w) => w !== op.worktree),
            sessions: t.sessions.filter((s) => s !== op.session),
            pending: t.pending.filter((p) => p.lane !== op.worktree),
          };
        }
        let next = { ...t, worktrees: uniq([...t.worktrees, op.worktree ?? '']), sessions: uniq([...t.sessions, op.session ?? '']) };
        if (op.session && !t.sessions.includes(op.session) && t.status === 'todo') next = withStatus(next, 'in-progress', now);
        return next;
      });
    case 'delete':
      return { ...store, todos: store.todos.filter((t) => t.id !== op.id) };
    case 'restore': {
      if (store.todos.some((t) => t.id === op.todo.id)) return store;
      const todos = [...store.todos];
      todos.splice(Math.min(op.index, todos.length), 0, op.todo);
      return { ...store, todos };
    }
    case 'order': {
      const rank = (id: string) => {
        const i = op.ids.indexOf(id);
        return i < 0 ? Number.MAX_SAFE_INTEGER : i;
      };
      return { ...store, todos: [...store.todos].sort((a, b) => rank(a.id) - rank(b.id)) };
    }
    case 'labels':
      return { ...store, labels: op.labels };
  }
}

/**
 * The todos and labels as the engine has them, with this window's changes on top until a board
 * that shows each one arrives. The engine answers a change with the time of that board; a
 * change it turned down goes, and `onError` hears why.
 */
export function useTodoStore(board: Board | null, onError: (message: string) => void) {
  const [changes, setChanges] = useState<{ seq: number; op: TodoOp; at: number; shownAt: number | null }[]>([]);
  const seq = useRef(0);
  const boardAt = board?.generatedAt ?? 0;
  const boardAtRef = useRef(boardAt);
  boardAtRef.current = boardAt;
  useEffect(() => {
    setChanges((cs) => {
      const left = cs.filter((c) => c.shownAt === null || c.shownAt > boardAt);
      return left.length === cs.length ? cs : left;
    });
  }, [boardAt]);
  const run = useCallback(
    (op: TodoOp) => {
      const id = ++seq.current;
      setChanges((cs) => [...cs, { seq: id, op, at: Date.now(), shownAt: null }]);
      todoOp(op)
        .then((shownAt) =>
          setChanges((cs) => (shownAt <= boardAtRef.current ? cs.filter((c) => c.seq !== id) : cs.map((c) => (c.seq === id ? { ...c, shownAt } : c)))),
        )
        .catch((e: unknown) => {
          setChanges((cs) => cs.filter((c) => c.seq !== id));
          onError(String(e).replace(/^Error: /, ''));
        });
    },
    [onError],
  );
  const store = useMemo(
    () => changes.reduce((s, c) => applyOp(s, c.op, c.at), { todos: board?.todos ?? [], labels: board?.labels ?? [] } as TodoStore),
    [board?.todos, board?.labels, changes],
  );
  return { ...store, run };
}

/** A session and the worktree it's listed under as its own. */
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

/** A worktree a todo is linked to, directly or through its sessions, with those sessions. */
export interface TodoPlace {
  lane: Lane;
  /** Linked itself, not only through a session. */
  linked: boolean;
  sessions: Session[];
}

/** A todo's row in the Todo view. */
export interface TodoItem {
  kind: 'todo';
  key: string;
  todo: Todo;
  /** The worktree its actions use: the first one linked, else its newest session's. */
  lane: Lane | null;
  places: TodoPlace[];
  /** Its linked sessions on the board, newest first. */
  sessions: Placed[];
  labels: Label[];
  /** The newest of its own changes and its sessions' activity. */
  lastActivity: number;
}

export function todoItem(todo: Todo, laneById: Map<string, Lane>, homes: Map<string, Placed>, labelById: Map<string, Label>): TodoItem {
  const sessions = todo.sessions.flatMap((k) => homes.get(k) ?? []).sort((a, b) => b.session.lastActivity - a.session.lastActivity);
  const places: TodoPlace[] = todo.worktrees.flatMap((id) => {
    const lane = laneById.get(id);
    return lane ? [{ lane, linked: true, sessions: [] as Session[] }] : [];
  });
  for (const { lane, session } of sessions) {
    let place = places.find((p) => p.lane.id === lane.id);
    if (!place) {
      place = { lane, linked: false, sessions: [] };
      places.push(place);
    }
    place.sessions.push(session);
  }
  const lane = places.find((p) => p.linked)?.lane ?? sessions[0]?.lane ?? null;
  const lastActivity = Math.max(todo.updatedAt, ...sessions.map((s) => s.session.lastActivity));
  return { kind: 'todo', key: `todo:${todo.id}`, todo, lane, places, sessions, labels: todo.labels.flatMap((id) => labelById.get(id) ?? []), lastActivity };
}

/** The PR of a todo's worktrees, open ones first. */
export function todoPr(item: TodoItem): Pr | null {
  const prs = item.places.flatMap((p) => (p.lane.pr ? [p.lane.pr] : []));
  return prs.find((p) => p.state !== 'merged' && p.state !== 'closed') ?? prs[0] ?? null;
}

export interface TodoSection {
  id: TodoStatus;
  label: string;
  rows: TodoItem[];
}

/** In progress and Todo in your order, then Done, the latest first. */
export function todoSections(items: TodoItem[]): TodoSection[] {
  const done = items.filter((r) => r.todo.status === 'done').sort((a, b) => (b.todo.doneAt ?? b.todo.updatedAt) - (a.todo.doneAt ?? a.todo.updatedAt));
  const sections: TodoSection[] = [
    { id: 'in-progress', label: 'In progress', rows: items.filter((r) => r.todo.status === 'in-progress') },
    { id: 'todo', label: 'Todo', rows: items.filter((r) => r.todo.status === 'todo') },
    { id: 'done', label: 'Done', rows: done },
  ];
  return sections.filter((s) => s.rows.length > 0);
}

/** Search: the title, the description, labels, and the worktrees and sessions it's linked to. */
export function todoInQuery(r: TodoItem, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  const hay = [
    r.todo.title,
    r.todo.description,
    ...r.labels.map((l) => l.name),
    ...r.places.map((p) => `${p.lane.callsign} ${p.lane.branch ?? ''}`),
    ...r.sessions.map((s) => `${s.session.account} ${s.session.title}`),
  ];
  return hay.some((h) => h.toLowerCase().includes(needle));
}

/** The Inbox filters, applied to a todo through its sessions and worktrees. */
export function todoInFilter(r: TodoItem, f: string | null): boolean {
  const has = (...st: Session['status'][]) => r.sessions.some(({ session }) => st.includes(session.status));
  switch (f) {
    case null:
      return true;
    case 'needs':
      return has('needs', 'error');
    case 'working':
    case 'unread':
      return has(f);
    case 'review': {
      const pr = todoPr(r);
      return !!pr && pr.state !== 'merged' && pr.state !== 'closed';
    }
    case 'failing':
      return r.places.some((p) => p.lane.pr && p.lane.pr.state !== 'merged' && p.lane.pr.state !== 'closed' && p.lane.pr.checks.failed > 0);
    default:
      return false;
  }
}

/** Which todos each session and each worktree is linked to, for their chips. A worktree counts
 *  its own links and its sessions'. */
export function todoIndex(todos: Todo[], homes: Map<string, Placed>): { bySession: Map<string, Todo[]>; byLane: Map<string, Todo[]> } {
  const bySession = new Map<string, Todo[]>();
  const byLane = new Map<string, Todo[]>();
  const add = (m: Map<string, Todo[]>, k: string, t: Todo) => {
    const list = m.get(k);
    if (!list) m.set(k, [t]);
    else if (!list.includes(t)) list.push(t);
  };
  for (const t of todos) {
    for (const w of t.worktrees) add(byLane, w, t);
    for (const k of t.sessions) {
      add(bySession, k, t);
      const home = homes.get(k);
      if (home) add(byLane, home.lane.id, t);
    }
  }
  return { bySession, byLane };
}

/** What a session started from a todo is asked: the title, then the description. */
export function todoPrompt(t: Todo): string {
  const title = t.title.trim();
  const body = t.description.trim();
  return body ? `${title}\n\n${body}` : title;
}

/** The first line of a description, for a row. Images and markup drop out. */
export function snippet(description: string, max = 140): string {
  const text = description
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

const IMAGE_NAME = /^[0-9a-f]{16}\.(png|jpg|gif|webp)$/;

/** A piece of a description as it's shown: text, or an image kept by Codealong. */
export type Piece = { kind: 'text'; text: string } | { kind: 'image'; name: string; alt: string };

/** Splits a description into text and the images it holds as `![alt](images/<name>)`. */
export function pieces(text: string): Piece[] {
  const out: Piece[] = [];
  const re = /!\[([^\]\n]*)\]\(images\/([^)\s]+)\)/g;
  let last = 0;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    if (!IMAGE_NAME.test(m[2])) continue;
    if (m.index > last) out.push({ kind: 'text', text: text.slice(last, m.index) });
    out.push({ kind: 'image', name: m[2], alt: m[1] });
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push({ kind: 'text', text: text.slice(last) });
  return out;
}

/** The images a text holds, in order, without repeats. */
export function imageNames(text: string): string[] {
  return [...new Set(pieces(text).flatMap((p) => (p.kind === 'image' ? [p.name] : [])))];
}

/** How text refers to a kept image. */
export function imageRef(name: string): string {
  return `![image](images/${name})`;
}

/** The text without an image's references, and without the blank lines they leave. */
export function withoutImage(text: string, name: string): string {
  const escaped = name.replace(/[.]/g, '\\.');
  return text
    .replace(new RegExp(`[ \\t]*!\\[[^\\]\\n]*\\]\\(images/${escaped}\\)[ \\t]*\\n?`, 'g'), '')
    .replace(/\n{3,}/g, '\n\n')
    .trimEnd();
}

/**
 * Puts an image reference into text at the cursor, on a line of its own. Returns the new text
 * and where the cursor goes, after the reference.
 */
export function insertImage(text: string, at: number, name: string): { text: string; cursor: number } {
  const before = text.slice(0, at);
  const after = text.slice(at);
  const lead = before === '' || before.endsWith('\n') ? '' : '\n';
  const tail = after === '' || after.startsWith('\n') ? '' : '\n';
  const ref = `${lead}${imageRef(name)}${tail}`;
  return { text: before + ref + after, cursor: before.length + ref.length };
}
