// Todos: work you mean to hand an agent. The engine keeps them in todos.json
// (src-tauri/src/todos.rs) and publishes them with the board. This file has the list's rules:
// its sections (In progress, In review, Ready, Waiting, Done), search and the status filters, the worktrees and sessions each todo is linked
// to and which of them is its primary session, the prompt a todo starts a session with, and the
// copy of each change a window shows until the engine's board has it.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { todoOp } from './api';
import { liveOrLatest, prNeedsWork, type Placed } from './lanes';
import type { Attachment, Board, Label, LabelColor, Lane, Pr, Session, Todo, TodoReview, TodoStatus, TodoSuggested, Waiting } from './types';
import { LABEL_COLORS } from './types';
import { errorText } from './report';

/** `dependsOn` replaces the whole list. `waiting: null` says it's ready; `review: null` sends it
 *  back to In progress; `due: null` takes the due date off. */
export type TodoPatch = Partial<Pick<Todo, 'title' | 'description' | 'status' | 'labels' | 'project' | 'dependsOn' | 'waiting' | 'review' | 'due'>>;

/** A change, as the engine takes it (`TodoOp` in todos.rs). */
export type TodoOp =
  | { op: 'create'; todo: Todo }
  | { op: 'update'; id: string; patch: TodoPatch }
  | { op: 'link'; id: string; worktree?: string; session?: string; linked: boolean }
  /** Makes a session the primary one, linking it; null leaves the todo without one. */
  | { op: 'primary'; id: string; session: string | null }
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
    dependsOn: [],
    waiting: null,
    review: null,
    due: null,
    ...fields,
  };
}

/** The color a new label gets: the first one no label has yet, else the next in turn. */
export function nextColor(labels: Label[]): LabelColor {
  const free = LABEL_COLORS.find((c) => c !== 'gray' && !labels.some((l) => l.color === c));
  return free ?? LABEL_COLORS[labels.length % LABEL_COLORS.length];
}

/** A status set the way the engine sets it: a done todo isn't waiting on anything, and only one
 *  in progress is in review. */
function withStatus(t: Todo, status: TodoStatus, now: number): Todo {
  const waiting = status === 'done' ? null : t.waiting;
  const review = status === 'in-progress' ? t.review : null;
  if (t.status === status) return waiting === t.waiting && review === t.review ? t : { ...t, waiting, review };
  return { ...t, status, doneAt: status === 'done' ? now : null, waiting, review };
}

/** A mark's time as the engine keeps it: a new note keeps the time it began, one put back by an
 *  undo keeps its own. */
function markSince(old: { since: number } | null | undefined, mark: { since: number }, now: number): number {
  return old && old.since > 0 ? old.since : mark.since > 0 && mark.since <= now ? mark.since : now;
}

const markNote = (note: string) => note.replace(/\s+/g, ' ').trim().slice(0, 200);

/** Marks a todo as waiting, or says it's ready, the way the engine does. A done todo can't be
 *  marked, and one waiting on something isn't with you to review. */
function withWaiting(t: Todo, w: Waiting | null, now: number): Todo {
  if (!w || t.status === 'done') return t.waiting ? { ...t, waiting: null } : t;
  return { ...t, waiting: { note: markNote(w.note), since: markSince(t.waiting, w, now) }, review: null };
}

/** Marks a todo as ready for review, or sends it back to In progress, the way the engine does:
 *  its work is under way from then on, and it waits on nothing outside the list. A done todo
 *  can't be marked. */
function withReview(t: Todo, r: TodoReview | null, now: number): Todo {
  if (!r || t.status === 'done') return t.review ? { ...t, review: null } : t;
  return { ...withStatus(t, 'in-progress', now), waiting: null, review: { note: markNote(r.note), since: markSince(t.review, r, now) } };
}

function uniq(list: string[]): string[] {
  return [...new Set(list.filter(Boolean))];
}

/** The ids a todo waits on. Boards from before dependencies have none. */
export function depsOf(t: Todo): string[] {
  return t.dependsOn ?? [];
}

/** A dependency list as the engine keeps it: each id once, not the todo itself. (The engine also
 *  refuses missing todos and loops; the refusal then takes this copy back.) */
function cleanDeps(id: string, list: string[]): string[] {
  return uniq(list).filter((d) => d !== id);
}

/** Whether `from` waits on `target`, directly or through the todos it waits on. */
export function waitsOn(from: string, target: string, todos: Todo[]): boolean {
  const byId = new Map(todos.map((t) => [t.id, t]));
  const stack = [from];
  const seen = new Set<string>();
  while (stack.length > 0) {
    const id = stack.pop()!;
    if (seen.has(id)) continue;
    seen.add(id);
    const t = byId.get(id);
    if (!t) continue;
    for (const d of depsOf(t)) {
      if (d === target) return true;
      stack.push(d);
    }
  }
  return false;
}

/** The todos `todo` waits on that aren't done, in its order. Deleted ones don't count. */
export function blockedBy(todo: Todo, todos: Todo[]): Todo[] {
  return depsOf(todo).flatMap((d) => todos.find((t) => t.id === d && t.status !== 'done') ?? []);
}

/** Where the Todo view lists a todo. */
export type TodoSectionId = 'in-progress' | 'review' | 'ready' | 'waiting' | 'done';

export const SECTION_WORD: Record<TodoSectionId, string> = { 'in-progress': 'In progress', review: 'In review', ready: 'Ready', waiting: 'Waiting', done: 'Done' };

/**
 * A todo's section: Waiting when it's marked as waiting, or when it isn't started and waits on
 * a todo that isn't done; In review when it's in progress and marked as ready for review, or
 * `prReview` says its PR is in review (`prInReview`); else by its status. One an agent is
 * already on stays In progress while the todos it waits on finish. `is_waiting` and
 * `is_in_review` in todos.rs have the same rules.
 */
export function sectionOf(todo: Todo, todos: Todo[], prReview = false): TodoSectionId {
  if (todo.status === 'done') return 'done';
  if (todo.waiting) return 'waiting';
  if (todo.status === 'in-progress') return todo.review || prReview ? 'review' : 'in-progress';
  return blockedBy(todo, todos).length > 0 ? 'waiting' : 'ready';
}

/** `sectionOf` for a todo's row, which knows its PR. */
export function itemSection(item: TodoItem, todos: Todo[]): TodoSectionId {
  return sectionOf(item.todo, todos, prInReview(item));
}

/** The todos `todo` waits on that still exist, done or not, in its order. */
export function waitsOnTodos(todo: Todo, todos: Todo[]): Todo[] {
  return depsOf(todo).flatMap((d) => todos.find((t) => t.id === d) ?? []);
}

/** The projects a todo can be in: every one but Elsewhere, which holds folders outside your
 *  projects. */
export function todoProjects(projects: { name: string }[]): string[] {
  return projects.filter((p) => p.name !== 'Elsewhere').map((p) => p.name);
}

/** What Mark as waiting or Stop waiting does to todos, from a menu or W in the Todo view. */
export interface WaitingChange {
  /** The item's name: Mark as waiting, else Stop waiting, on what it waits on when that's one
   *  todo not started. */
  label: string;
  ops: TodoOp[];
  /** What puts back what stopping changed; nothing when it marked them. */
  undo: TodoOp[];
  /** What stopping changed, in a sentence; null when it marked them. */
  said: string | null;
}

/**
 * Moving todos into Waiting by hand, or back out (`todos` is every todo, since what one waits on
 * is others). Any not done and not under Waiting are marked; when all are, stopping ends each
 * one's mark and its waits on todos that aren't done, so it's ready at once. Null when they're
 * all done.
 */
export function waitingChange(list: Todo[], todos: Todo[]): WaitingChange | null {
  const ts = list.filter((t) => t.status !== 'done');
  if (ts.length === 0) return null;
  const out = ts.filter((t) => sectionOf(t, todos) !== 'waiting');
  if (out.length > 0) return { label: 'Mark as waiting', ops: out.map((t): TodoOp => ({ op: 'update', id: t.id, patch: { waiting: { note: '', since: 0 } } })), undo: [], said: null };
  // One an agent is on leaves Waiting with its mark; one not started, only once it waits on
  // nothing.
  const stopped = ts.map((t) => {
    const open = t.status === 'todo' ? blockedBy(t, todos) : [];
    const dependsOn = depsOf(t).filter((d) => !open.some((o) => o.id === d));
    const op: TodoOp = { op: 'update', id: t.id, patch: { waiting: null, ...(open.length > 0 ? { dependsOn } : {}) } };
    const undo: TodoOp = { op: 'update', id: t.id, patch: { waiting: t.waiting ?? null, dependsOn: depsOf(t) } };
    return { t, open, op, undo };
  });
  const [{ t, open }] = stopped;
  const on = open.length === 0 ? '' : open.length === 1 ? ` on “${open[0].title || 'Untitled'}”` : ` on ${open.length} todos`;
  const label = stopped.length > 1 || t.waiting || open.length === 0 ? 'Stop waiting' : open.length === 1 ? `Stop waiting on “${snippet(open[0].title || 'Untitled', 44)}”` : `Stop waiting on ${open.length} todos`;
  return {
    label,
    ops: stopped.map((x) => x.op),
    undo: stopped.map((x) => x.undo),
    said: stopped.length === 1 ? `“${t.title || 'Untitled'}” no longer waits${on}.` : `${stopped.length} todos no longer wait.`,
  };
}

/** Ready for review for the open todos of `list` not marked so, or Back to in progress once they
 *  all are: what a menu or R does to them. */
export function reviewChange(list: Todo[]): { label: string; ops: TodoOp[] } | null {
  const ts = list.filter((t) => t.status !== 'done');
  if (ts.length === 0) return null;
  if (ts.every((t) => t.review)) return { label: 'Back to in progress', ops: ts.map((t) => ({ op: 'update', id: t.id, patch: { review: null } })) };
  return { label: 'Ready for review', ops: ts.filter((t) => !t.review).map((t) => ({ op: 'update', id: t.id, patch: { review: { note: '', since: 0 } } })) };
}

/** The todos that wait on `todo`, in list order. */
export function dependents(todo: Todo, todos: Todo[]): Todo[] {
  return todos.filter((t) => t.id !== todo.id && depsOf(t).includes(todo.id));
}

/** Why `todo` can't wait on `other`, or null when it can: not itself, not a done one, not one
 *  already on its list, and not one that already waits on it (that would be a loop). */
export function cantWaitOn(todo: Todo, other: Todo, todos: Todo[]): string | null {
  if (other.id === todo.id) return 'This todo';
  if (depsOf(todo).includes(other.id)) return 'Already waits on it';
  if (other.status === 'done') return 'Done';
  if (waitsOn(other.id, todo.id, todos)) return 'Waits on this one';
  return null;
}

/** Jev's suggestions worth a chip: labels it isn't wearing yet, a project it isn't in, and
 *  todos it could wait on, each only when Jev is sure enough. Nothing is applied until you
 *  click. */
export function todoSuggestions(s: TodoSuggested, todo: Todo, labels: Label[], todos: Todo[], projects: string[]): { project: string | null; labels: Label[]; dependsOn: Todo[] } {
  const [top] = s.projects;
  const project = top && top[1] >= 0.7 && top[0] !== todo.project && projects.includes(top[0]) ? top[0] : null;
  const chosenLabels = s.labels.filter(([id, p]) => p >= 0.6 && !todo.labels.includes(id)).flatMap(([id]) => labels.find((l) => l.id === id) ?? []);
  const deps = s.dependsOn.filter(([, p]) => p >= 0.7).flatMap(([id]) => todos.find((t) => t.id === id) ?? []).filter((t) => cantWaitOn(todo, t, todos) === null);
  return { project, labels: chosenLabels, dependsOn: deps };
}

/** Whether a todo says enough to ask about: a few words in its title, or a title with a
 *  description (such as a screenshot). */
export function worthSuggesting(todo: Todo): boolean {
  const title = todo.title.trim();
  if (!title) return false;
  return title.split(/\s+/).length >= 3 || todo.description.trim().length > 0;
}

/** A change applied the way the engine applies it, so a window can show it at once. */
export function applyOp(store: TodoStore, op: TodoOp, now: number): TodoStore {
  const map = (id: string, f: (t: Todo) => Todo) => ({ ...store, todos: store.todos.map((t) => (t.id === id ? f(t) : t)) });
  switch (op.op) {
    case 'create': {
      if (store.todos.some((t) => t.id === op.todo.id)) return store;
      let t: Todo = { ...op.todo, worktrees: uniq(op.todo.worktrees), sessions: uniq(op.todo.sessions), pending: [], dependsOn: cleanDeps(op.todo.id, depsOf(op.todo)), waiting: null, review: null };
      // The engine keeps the clock: a new todo's wait, or review, begins now.
      if (op.todo.waiting) t = withWaiting(t, { ...op.todo.waiting, since: 0 }, now);
      if (op.todo.review) t = withReview(t, { ...op.todo.review, since: 0 }, now);
      if (t.sessions.length > 0 && t.status === 'todo') t = withStatus(t, 'in-progress', now);
      return { ...store, todos: [t, ...store.todos] };
    }
    case 'update':
      return map(op.id, (t) => {
        const { status, waiting, review, ...rest } = op.patch;
        let next: Todo = { ...t, ...rest, updatedAt: now };
        if (rest.dependsOn) next.dependsOn = cleanDeps(t.id, rest.dependsOn);
        if (status) {
          next = withStatus(next, status, now);
          // Saying an agent is on it says it's no longer waiting, nor waiting on you.
          if (status === 'in-progress' && (next.waiting || next.review)) next = { ...next, waiting: null, review: null };
        }
        if (waiting !== undefined) next = withWaiting(next, waiting, now);
        return review === undefined ? next : withReview(next, review, now);
      });
    case 'link':
      return map(op.id, (t) => {
        if (!op.linked) {
          return {
            ...t,
            worktrees: t.worktrees.filter((w) => w !== op.worktree),
            sessions: t.sessions.filter((s) => s !== op.session),
            pending: t.pending.filter((p) => p.lane !== op.worktree),
            primary: t.primary && t.primary.session === op.session ? null : t.primary,
          };
        }
        return linked(t, op.worktree, op.session, now);
      });
    case 'primary':
      return map(op.id, (t) => {
        if (!op.session) return { ...t, primary: null, updatedAt: now };
        const next = linked(t, undefined, op.session, now);
        return next.primary?.session === op.session ? next : { ...next, primary: { session: op.session, since: now }, updatedAt: now };
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

/** A todo with a worktree or session linked. The first session on it is its primary one; when
 *  the engine has another session take it over from one that stopped, its board says so. */
function linked(t: Todo, worktree: string | undefined, session: string | undefined, now: number): Todo {
  let next: Todo = { ...t, worktrees: uniq([...t.worktrees, worktree ?? '']), sessions: uniq([...t.sessions, session ?? '']) };
  if (session && !t.sessions.includes(session) && t.status === 'todo') next = withStatus(next, 'in-progress', now);
  if (session && !t.primary) next = { ...next, primary: { session, since: now } };
  return next;
}

/**
 * The todos and labels as the engine has them, with this window's changes on top until a board
 * that shows each one arrives. The engine answers a change with the time of that board; a
 * change it turned down goes, and `onError` hears why. `run` resolves once the engine has
 * answered, for changes that must reach it in turn.
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
    (op: TodoOp): Promise<void> => {
      const id = ++seq.current;
      setChanges((cs) => [...cs, { seq: id, op, at: Date.now(), shownAt: null }]);
      return todoOp(op)
        .then((shownAt) =>
          setChanges((cs) => (shownAt <= boardAtRef.current ? cs.filter((c) => c.seq !== id) : cs.map((c) => (c.seq === id ? { ...c, shownAt } : c)))),
        )
        .catch((e: unknown) => {
          setChanges((cs) => cs.filter((c) => c.seq !== id));
          onError(errorText(e));
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
  /** Its primary session while that runs (`primaryHolds`), where the board lists it. */
  primary: Placed | null;
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
  const primary = sessions.find((s) => s.session.key === todo.primary?.session && running(s.session)) ?? null;
  return { kind: 'todo', key: `todo:${todo.id}`, todo, lane, places, sessions, primary, labels: todo.labels.flatMap((id) => labelById.get(id) ?? []), lastActivity };
}

/** How long a session made primary holds its todo before the board lists it (`NEW_PRIMARY_MS`
 *  in todos.rs): one started in a terminal shows up a few seconds after it's linked. */
const NEW_PRIMARY_MS = 5 * 60 * 1000;

/** Running, as a primary session has to be to hold its todo: its process is up and it isn't
 *  archived. */
function running(s: Session): boolean {
  return s.live && !s.archived;
}

/** Whether a todo's primary session still has it: it's running, or it became primary so lately
 *  that the board doesn't list it yet. `holds` in todos.rs has the same rule. */
export function primaryHolds(todo: Todo, homes: Map<string, Placed>, now: number): boolean {
  if (!todo.primary) return false;
  const home = homes.get(todo.primary.session);
  return home ? running(home.session) : now - todo.primary.since < NEW_PRIMARY_MS;
}

/** `primaryHolds` for a todo's row, whose sessions are the ones on the board linked to it. */
export function itemPrimaryHolds(item: Pick<TodoItem, 'todo' | 'sessions'>, now: number): boolean {
  return primaryHolds(item.todo, new Map(item.sessions.map((s) => [s.session.key, s])), now);
}

/** The session to open for a todo: its primary one while that runs, else the running one, else
 *  the latest. */
export function todoAgent(item: Pick<TodoItem, 'primary' | 'sessions'>): Placed | null {
  return item.primary ?? liveOrLatest(item.sessions);
}

/** The PR of a todo's worktrees, open ones first. */
export function todoPr(item: TodoItem): Pr | null {
  // A worktree it's linked to, or one a session on it works in while that session is on the
  // worktree's branch, as lanes.ts `sessionPrs` has it: a checkout gone on to other work has
  // that work's PR. `pr_in_review` in todos.rs picks the same.
  const prs = item.places.flatMap((p) => (p.lane.pr && (p.linked || p.sessions.some((s) => (s.branch ?? p.lane.branch) === p.lane.branch)) ? [p.lane.pr] : []));
  return prs.find((p) => p.state !== 'merged' && p.state !== 'closed') ?? prs[0] ?? null;
}

/** Whether a todo's PR (`todoPr`) is with its reviewers: open and not a draft, or merged while the
 *  todo isn't done, which waits on you to say it is. `pr_in_review` in todos.rs has the same
 *  rule. */
export function prInReview(item: TodoItem): boolean {
  const state = todoPr(item)?.state;
  return state === 'open' || state === 'approved' || state === 'changes' || state === 'merged';
}

export interface TodoSection {
  id: TodoSectionId;
  label: string;
  rows: TodoItem[];
}

/** In progress, In review, Ready and Waiting in your order, then Done, the latest first. `todos`
 *  is every todo, in any project: what a todo waits on can be anywhere. */
export function todoSections(items: TodoItem[], todos: Todo[]): TodoSection[] {
  const rows = new Map<TodoSectionId, TodoItem[]>([
    ['in-progress', []],
    ['review', []],
    ['ready', []],
    ['waiting', []],
    ['done', []],
  ]);
  for (const r of items) rows.get(itemSection(r, todos))?.push(r);
  rows.get('done')?.sort((a, b) => (b.todo.doneAt ?? b.todo.updatedAt) - (a.todo.doneAt ?? a.todo.updatedAt));
  return [...rows].filter(([, list]) => list.length > 0).map(([id, list]) => ({ id, label: SECTION_WORD[id], rows: list }));
}

/** The status filters, applied to a todo through its sessions and worktrees. */
export function todoInFilter(r: TodoItem, fs: readonly string[]): boolean {
  return fs.length === 0 || fs.some((f) => todoIn(r, f));
}

function todoIn(r: TodoItem, f: string): boolean {
  const has = (...st: Session['status'][]) => r.sessions.some(({ session }) => st.includes(session.status));
  switch (f) {
    case 'needs':
      return has('needs', 'error');
    case 'working':
    case 'unread':
      return has(f);
    case 'review': {
      // Its PR open, a draft too, as the other views have it; or under In review, marked or merged.
      const pr = todoPr(r);
      return (!!pr && pr.state !== 'merged' && pr.state !== 'closed') || (r.todo.status === 'in-progress' && !r.todo.waiting && (!!r.todo.review || prInReview(r)));
    }
    case 'work':
      return r.places.some((p) => prNeedsWork(p.lane.pr));
    default:
      return false;
  }
}

/** Which todos each session and each worktree is linked to, for their chips: `byLane` counts a
 *  worktree's own links and its sessions', `byWorktree` only its own. */
export function todoIndex(todos: Todo[], homes: Map<string, Placed>): { bySession: Map<string, Todo[]>; byLane: Map<string, Todo[]>; byWorktree: Map<string, Todo[]> } {
  const bySession = new Map<string, Todo[]>();
  const byLane = new Map<string, Todo[]>();
  const byWorktree = new Map<string, Todo[]>();
  const add = (m: Map<string, Todo[]>, k: string, t: Todo) => {
    const list = m.get(k);
    if (!list) m.set(k, [t]);
    else if (!list.includes(t)) list.push(t);
  };
  for (const t of todos) {
    for (const w of t.worktrees) {
      add(byLane, w, t);
      add(byWorktree, w, t);
    }
    for (const k of t.sessions) {
      add(bySession, k, t);
      const home = homes.get(k);
      if (home) add(byLane, home.lane.id, t);
    }
  }
  return { bySession, byLane, byWorktree };
}

/** A session's linked todos as its panel lists them: the ones still under way, then the done
 *  ones, latest first, so the few shown before "more" are the ones that matter now. */
export function panelTodos(todos: Todo[]): Todo[] {
  const rank = (t: Todo) => (t.status === 'in-progress' ? 0 : t.status === 'todo' ? 1 : 2);
  return [...todos].sort((a, b) => rank(a) - rank(b) || (a.status === 'done' ? (b.doneAt ?? 0) - (a.doneAt ?? 0) : 0));
}

/** What a session started from a todo is asked: the title, the description, then which todo
 *  it is, so the agent can mark it done through the codealong skill, and its primary session
 *  (`primaryLine`) when `holding` says whether that one still has it. `todo_prompt` in cli.rs
 *  writes the same text. */
export function todoPrompt(t: Todo, holding?: boolean): string {
  const title = t.title.trim();
  const body = t.description.trim();
  const footer = `This is Codealong todo ${t.id}. When it's ready for me to review, say so with \`codealong todo review ${t.id}\`. When the work is merged or I say it's done, mark it done with \`codealong todo done ${t.id}\`.`;
  return [title, body, footer, holding === undefined ? null : primaryLine(t, holding)].filter(Boolean).join('\n\n');
}

/** What a session started on a todo is told of the todo's primary session: that it's running,
 *  so this one sends it what it finds; or that it had the todo before, where its work is.
 *  `primary_line` in cli.rs writes the same text. */
export function primaryLine(t: Todo, holding: boolean): string | null {
  const key = t.primary?.session;
  if (!key) return null;
  return holding
    ? `${t.id}'s primary session is ${key}, which is running: send it what you find with \`codealong session send ${key} "…"\` rather than do the same work.`
    : `Session ${key} had ${t.id} before you: \`codealong session show ${key}\` has its records.`;
}

/** What one session started for several todos is asked: each one's title and description,
 *  numbered, then which todos they are, and their primary sessions when `holding` is given.
 *  One todo is `todoPrompt`'s. `todos_prompt` in cli.rs writes the same text. */
export function todosPrompt(ts: Todo[], holding?: (t: Todo) => boolean): string {
  if (ts.length === 1) return todoPrompt(ts[0], holding?.(ts[0]));
  const parts = ts.flatMap((t, i) => [`${i + 1}. ${t.title.trim() || 'Untitled'}`, t.description.trim()].filter(Boolean));
  const ids = ts.map((t, i) => `${t.id} (${i + 1})`);
  const footer = `These are Codealong todos ${ids.slice(0, -1).join(', ')} and ${ids[ids.length - 1]}. When one is ready for me to review, say so with \`codealong todo review <id>\`. When the work on one is merged or I say it's done, mark it done with \`codealong todo done <id>\`.`;
  const primaries = holding ? ts.flatMap((t) => primaryLine(t, holding(t)) ?? []) : [];
  return [`Work on these ${ts.length} todos:`, ...parts, footer, ...primaries].join('\n\n');
}

/** The first line of a description, for a row. Images drop out; a file leaves its name. */
export function snippet(description: string, max = 140): string {
  const text = description
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
    .replace(FILE_LINK, (_, name: string) => ` ${name} `)
    .replace(/\s+/g, ' ')
    .trim();
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

const IMAGE_NAME = /^[0-9a-f]{16}\.(png|jpg|gif|webp)$/;
/** Where attachments.rs keeps a file and how it names it: `files/<hash>/<name>`. */
const FILE_PATH = String.raw`files\/[0-9a-f]{16}\/[^)\s/]+`;
/** `[name](files/<hash>/<name>)`, as attachments.rs keeps and names them. */
const FILE_LINK = new RegExp(String.raw`(?<!!)\[([^\]\n]*)\]\((${FILE_PATH})\)`, 'g');
const KEPT_FILE = new RegExp(`^${FILE_PATH}$`);

/** Whether a link goes to a file Codealong keeps (`files/<hash>/<name>`). */
export function isKeptFile(target: string): boolean {
  return KEPT_FILE.test(target);
}

/** The name of the image Codealong keeps that an image's link goes to (`images/<name>`), or null. */
export function keptImageOf(target: string): string | null {
  const name = target.startsWith('images/') ? target.slice('images/'.length) : '';
  return IMAGE_NAME.test(name) ? name : null;
}

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

/** A run of text as it's shown: words, a web link, or a kept file. */
export type Inline = { kind: 'text'; text: string } | { kind: 'url'; url: string } | { kind: 'file'; name: string; path: string };

const URL_RE = /https?:\/\/[^\s<>()]+[^\s<>().,;:!?'"]/g;

/** Splits text into words, web links and file links, so each can be shown for what it is. */
export function inlines(text: string): Inline[] {
  const found: { at: number; end: number; part: Inline }[] = [];
  for (const m of text.matchAll(FILE_LINK)) found.push({ at: m.index, end: m.index + m[0].length, part: { kind: 'file', name: m[1] || m[2].split('/').pop() || 'file', path: m[2] } });
  for (const m of text.matchAll(URL_RE)) {
    if (!found.some((f) => m.index < f.end && m.index + m[0].length > f.at)) found.push({ at: m.index, end: m.index + m[0].length, part: { kind: 'url', url: m[0] } });
  }
  found.sort((a, b) => a.at - b.at);
  const out: Inline[] = [];
  let last = 0;
  for (const f of found) {
    if (f.at > last) out.push({ kind: 'text', text: text.slice(last, f.at) });
    out.push(f.part);
    last = f.end;
  }
  if (last < text.length) out.push({ kind: 'text', text: text.slice(last) });
  return out;
}

/** The attachments a text holds, images and files, in order, without repeats. */
export function attachmentsIn(text: string): Attachment[] {
  const found: { at: number; a: Attachment }[] = [];
  for (const m of text.matchAll(/!\[([^\]\n]*)\]\((images\/([^)\s]+))\)/g)) {
    if (IMAGE_NAME.test(m[3])) found.push({ at: m.index, a: { kind: 'image', path: m[2], name: m[1] || 'image' } });
  }
  for (const m of text.matchAll(FILE_LINK)) found.push({ at: m.index, a: { kind: 'file', path: m[2], name: m[1] || m[2].split('/').pop() || 'file' } });
  const seen = new Set<string>();
  return found
    .sort((x, y) => x.at - y.at)
    .flatMap(({ a }) => {
      if (seen.has(a.path)) return [];
      seen.add(a.path);
      return [a];
    });
}

/** How text refers to a kept attachment: an image shows in place, a file is a link. */
export function attachmentRef(a: Attachment): string {
  return a.kind === 'image' ? `![image](${a.path})` : `[${a.name.replace(/[[\]\n]/g, '')}](${a.path})`;
}

/** The text without an attachment's references, and without the blank lines they leave. */
export function withoutAttachment(text: string, path: string): string {
  const escaped = path.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return text
    .replace(new RegExp(`([ \\t]*)(!?\\[[^\\]\\n]*\\]\\(${escaped}\\))([ \\t]*)(\\n?)`, 'g'), (match, lead: string, _ref: string, tail: string, nl: string, offset: number, str: string) => {
      const atLineStart = offset === 0 || str[offset - 1] === '\n';
      const atLineEnd = nl !== '' || offset + match.length === str.length;
      if (atLineStart && atLineEnd) return '';
      if (atLineStart) return '';
      if (atLineEnd) return nl;
      return lead || tail ? ' ' : '';
    })
    .replace(/\n{3,}/g, '\n\n')
    .trimEnd();
}

/**
 * Puts attachments into text at the cursor, each on a line of its own. Returns the new text and
 * where the cursor goes, after the last one.
 */
export function insertAttachments(text: string, at: number, kept: Attachment[]): { text: string; cursor: number } {
  let out = text;
  let cursor = Math.min(at, text.length);
  for (const a of kept) {
    const before = out.slice(0, cursor);
    const after = out.slice(cursor);
    const lead = before === '' || before.endsWith('\n') ? '' : '\n';
    const tail = after === '' || after.startsWith('\n') ? '' : '\n';
    const ref = `${lead}${attachmentRef(a)}${tail}`;
    out = before + ref + after;
    cursor = before.length + ref.length;
  }
  return { text: out, cursor };
}

/** Splits text into its words and the attachments it holds, for a composer that shows
 *  attachments in a strip rather than as markdown in its text box. */
export function splitAttachments(text: string): { text: string; attachments: Attachment[] } {
  const attachments = attachmentsIn(text);
  let words = text;
  for (const a of attachments) words = withoutAttachment(words, a.path);
  return { text: words.replace(/^\n+/, ''), attachments };
}

/** Joins a composer's words with its attached images and files, each on a line of its own at
 *  the end, without repeating any the words still name. */
export function withAttachments(text: string, attachments: Attachment[]): string {
  const refs = attachments.filter((a) => !text.includes(`(${a.path})`)).map(attachmentRef);
  if (refs.length === 0) return text;
  const trimmed = text.trimEnd();
  return trimmed ? `${trimmed}\n\n${refs.join('\n')}` : refs.join('\n');
}

/** Adds newly kept attachments to a list in order, without repeats. */
export function mergeAttachments(current: Attachment[], added: Attachment[]): Attachment[] {
  const seen = new Set(current.map((a) => a.path));
  const next = [...current];
  for (const a of added) {
    if (!seen.has(a.path)) {
      seen.add(a.path);
      next.push(a);
    }
  }
  return next;
}

