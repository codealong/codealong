// The Todo view's own state and what's done to todos from the main window: the todos picked
// together, the box that adds one, the due date a panel opens its picker on, and every change,
// menu and key, each going through the todo store (todos.ts), which shows a change at once. The
// filters it shares with the other views (the label, Show done) stay with them in App.tsx, as
// the lists are made from them (boardLists.ts).

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ListRow } from './boardLists';
import { showContextMenu, type MenuSpec } from './contextMenu';
import { type Filter } from './lanes';
import { useLatest } from './latest';
import * as menus from './menus';
import { nextAfter, usePicks } from './picks';
import { reorderPinned, type Pins } from './pins';
import { ALL_PROJECTS, inProject } from './quick';
import type { Report } from './report';
import { COLOR_WORD, itemPrimaryHolds, newTodo, todoAgent, todoProjects, todoPrompt, todosPrompt, useTodoStore, type TodoItem, type TodoSection, type TodoSectionId } from './todos';
import type { Label, Lane, ProjectSummary, Todo, TodoStatus } from './types';
import { LABEL_COLORS } from './types';
import type { UndoToast } from './ui/Toasts';
import type { ComposerContext } from './ui/TodoView';

/** What the Todo view goes by, as the window has it. */
export interface TodoViewInput {
  store: ReturnType<typeof useTodoStore>;
  view: string;
  selected: ListRow | null;
  setSelectedKey: (key: string | null) => void;
  visible: ListRow[];
  todoVisible: ListRow[];
  todoSecs: TodoSection[];
  current: string;
  projects: ProjectSummary[];
  lanes: Lane[];
  laneById: Map<string, Lane>;
  lanePins: Pins;
  selectedLane: Lane | null;
  labelFilter: string | null;
  setLabelFilter: (id: string | null) => void;
  setShowDoneTodos: (show: boolean) => void;
  report: Report;
  setToast: (message: string | null) => void;
  setTodoUndo: (undo: UndoToast | null) => void;
  newSession: (at: { laneId: string | null; todo?: string | null; todos?: string[]; prompt?: string }) => void;
  /** Opens one of a worktree's targets where it lives. */
  open: (lane: Lane, targetId: string | null | undefined) => void;
  pickProject: (name: string) => void;
  setView: (view: 'todo') => void;
  setBayFilter: (filter: readonly Filter[]) => void;
  setQuery: (query: string) => void;
  onDesktop: ReadonlySet<string>;
  /** Add to canvas, or Show on canvas, for a todo's menu. */
  canvasItem: (ref: 'todo', key: string) => MenuSpec;
}

export function useTodoView(input: TodoViewInput) {
  const { store: todoStore, view, selected, setSelectedKey, visible, todoVisible, todoSecs, current, projects, lanes, laneById, lanePins, selectedLane, labelFilter, setLabelFilter, setShowDoneTodos, report, setToast, setTodoUndo, newSession, open, pickProject, setView, setBayFilter, setQuery, onDesktop, canvasItem } = input;
  const runTodo = todoStore.run;
  // The todo whose panel should open its due date's picker, for Pick a date… and D.
  const [duePick, setDuePick] = useState<string | null>(null);
  const duePicked = useCallback(() => setDuePick(null), []);
  // The box that adds todos, and what a new one gets linked to.
  const [composer, setComposer] = useState<{ text: string; context: ComposerContext | null }>({ text: '', context: null });
  const composerRef = useRef<HTMLInputElement>(null);
  const [composerFocus, setComposerFocus] = useState(0);

  useEffect(() => {
    if (composerFocus === 0) return;
    composerRef.current?.focus();
    composerRef.current?.select();
  }, [composerFocus]);

  // Todos picked together with the selected one (⌘-click, ⇧-click, ⇧↑ ⇧↓), to start in one
  // session (picks.ts).
  const todoRows = useMemo(() => (view === 'todo' ? todoVisible.filter((r): r is TodoItem => r.kind === 'todo') : null), [view, todoVisible]);
  const picks = usePicks(todoRows, selected?.key ?? null, setSelectedKey);
  const { picked: pickedRows, keys: pickedKeys, select: selectTodoRow, clear: clearPick } = picks;

  /** Opens the quick panel for a todo: in its worktree, with the todo as the prompt. */
  const startForTodo = useCallback(
    (r: TodoItem) => newSession({ laneId: r.lane?.id ?? selectedLane?.id ?? null, todo: r.todo.id, prompt: todoPrompt(r.todo, itemPrimaryHolds(r, Date.now())) }),
    [newSession, selectedLane?.id],
  );

  /** Opens the quick panel for one session on several todos: in the worktree of the first that
   *  has one, on a prompt that has each of them. */
  const startForTodos = useCallback((rows: TodoItem[]) => {
    if (rows.length === 1) return startForTodo(rows[0]);
    const now = Date.now();
    const holding = (t: Todo) => rows.some((r) => r.todo === t && itemPrimaryHolds(r, now));
    newSession({ laneId: rows.find((r) => r.lane)?.lane?.id ?? selectedLane?.id ?? null, todos: rows.map((r) => r.todo.id), prompt: todosPrompt(rows.map((r) => r.todo), holding) });
  }, [startForTodo, newSession, selectedLane?.id]);

  /** Shows a todo in the Todo view, in its project, with nothing hiding it. */
  const showTodo = (t: Todo) => {
    const home = t.project && projects.some((p) => p.name === t.project) ? t.project : ALL_PROJECTS;
    if (current !== ALL_PROJECTS && current !== home) pickProject(home);
    setView('todo');
    setBayFilter([]);
    setLabelFilter(null);
    setQuery('');
    if (t.status === 'done') setShowDoneTodos(true);
    setSelectedKey(`todo:${t.id}`);
  };

  /** Adds a todo from the box at the top of the list, linked to what it was made from. */
  const createTodo = (title: string, context: ComposerContext | null) => {
    const lane = context?.worktree ? laneById.get(context.worktree) : undefined;
    const project = current !== ALL_PROJECTS ? current : (lane?.project ?? '');
    const t = newTodo(
      {
        title,
        project,
        worktrees: context?.worktree ? [context.worktree] : [],
        sessions: context?.session ? [context.session] : [],
        // Made while a label filter is on, it takes the label, so it stays in view.
        labels: labelFilter ? [labelFilter] : [],
      },
      Date.now(),
    );
    runTodo({ op: 'create', todo: t });
    setSelectedKey(`todo:${t.id}`);
    setComposer({ text: '', context: null });
  };

  /** Opens the Todo view with the box at the top ready for a todo linked to `context`. */
  const newTodoFrom = (context: ComposerContext, title = '') => {
    setView('todo');
    setBayFilter([]);
    setComposer({ text: title, context });
    setComposerFocus((n) => n + 1);
  };

  const setTodoStatus = (t: Todo, status: TodoStatus) => runTodo({ op: 'update', id: t.id, patch: { status } });

  /** The projects a todo can be in: every one but Elsewhere. */
  const projectsForTodos = useMemo(() => todoProjects(projects), [projects]);

  /** How this window changes todos, for their menus (menus.ts): through the store, which shows a
   *  change at once, with an undo for stopping waits and the panel's picker for any day. */
  const todoEdit: menus.TodoEdit = {
    run: runTodo,
    todos: todoStore.todos,
    labels: todoStore.labels,
    projects: projectsForTodos,
    pickDate: (t) => {
      showTodo(t);
      setDuePick(t.id);
    },
    stopped: (message, undo) => setTodoUndo({ message, icon: 'hourglass', undo: () => undo.forEach((op) => void runTodo(op)) }),
  };

  /** Moves todos to Waiting by hand, or back out (`waitingChange`); the toast can put a stop back. */
  const toggleWaiting = (list: Todo[]) => menus.toggleWaiting(list, todoEdit, report);

  /** Deletes todos; the toast can put them back where they were. */
  const deleteTodos = (ts: Todo[]) => {
    if (ts.length === 0) return;
    const index = new Map(ts.map((t) => [t.id, todoStore.todos.findIndex((x) => x.id === t.id)]));
    const keys = new Set(ts.map((t) => `todo:${t.id}`));
    // The selection moves to the row after the last one deleted, else the one before the first.
    if (selected && keys.has(selected.key)) setSelectedKey(nextAfter(todoVisible.map((r) => r.key), keys));
    clearPick();
    for (const t of ts) runTodo({ op: 'delete', id: t.id });
    // They go back one after another, first to last, so each lands where it was.
    const back = [...ts].sort((a, b) => (index.get(a.id) ?? 0) - (index.get(b.id) ?? 0));
    setTodoUndo({
      message: ts.length === 1 ? `Deleted “${ts[0].title || 'Untitled'}”.` : `Deleted ${ts.length} todos.`,
      undo: () => void back.reduce((done, t) => done.then(() => runTodo({ op: 'restore', todo: t, index: index.get(t.id) ?? 0 })), Promise.resolve()),
    });
  };

  /** Moves a todo to insertion point `index` among the rows its section shows. */
  const moveTodo = (section: TodoSectionId, id: string, index: number) => {
    const shown = todoSecs.find((s) => s.id === section)?.rows.map((r) => r.todo.id) ?? [];
    const all = todoStore.todos.map((t) => t.id);
    const next = reorderPinned(all, shown, id, index);
    if (next !== all) runTodo({ op: 'order', ids: next });
  };

  const labelsRef = useLatest(todoStore.labels);
  /** Deletes a label everywhere; the toast can bring it back where it was. */
  const deleteLabel = (label: Label) => {
    const at = todoStore.labels.findIndex((l) => l.id === label.id);
    runTodo({ op: 'labels', labels: todoStore.labels.filter((l) => l.id !== label.id) });
    if (labelFilter === label.id) setLabelFilter(null);
    setTodoUndo({
      message: `Deleted the label “${label.name}”.`,
      undo: () => {
        const labels = labelsRef.current.filter((l) => l.id !== label.id);
        labels.splice(Math.min(at, labels.length), 0, label);
        runTodo({ op: 'labels', labels });
      },
    });
  };

  /** A label's right-click menu, in the label picker: its color, rename, delete. */
  const labelMenu = (label: Label, rename: () => void) => {
    void showContextMenu([
      {
        label: 'Color',
        items: LABEL_COLORS.map((c) => ({
          label: COLOR_WORD[c],
          checked: label.color === c,
          run: () => runTodo({ op: 'labels', labels: labelsRef.current.map((l) => (l.id === label.id ? { ...l, color: c } : l)) }),
        })),
      },
      { label: 'Rename…', run: rename },
      'separator',
      { label: 'Delete label', run: () => deleteLabel(label) },
    ]).catch((e: unknown) => setToast(String(e)));
  };

  /** A todo's right-click menu. */
  const todoMenu = (r: TodoItem) => void showContextMenu(todoMenuSpec(r)).catch((e: unknown) => setToast(String(e)));
  const todoMenuSpec = (r: TodoItem): MenuSpec => {
    const t = r.todo;
    const agent = todoAgent(r);
    return menus.todoMenu(t, {
      report,
      start: () => startForTodo(r),
      session: agent ? { ...agent, open: () => open(agent.lane, agent.session.openTarget) } : undefined,
      edit: todoEdit,
      desktop: onDesktop,
      canvas: canvasItem('todo', t.id),
      remove: () => deleteTodos([t]),
    });
  };

  /** The right-click menu of several todos picked together, and their panel's ⋯ menu. */
  const pickMenuSpec = (rows: TodoItem[]): MenuSpec => {
    const ts = rows.map((r) => r.todo);
    return menus.todosMenu(ts, { report, start: () => startForTodos(rows), edit: todoEdit, remove: () => deleteTodos(ts) });
  };
  const pickMenu = (rows: TodoItem[], at?: { x: number; y: number }) => void showContextMenu(pickMenuSpec(rows), at).catch((e: unknown) => setToast(String(e)));

  /** The panel's ⋯ menu. */
  const todoMoreMenu = (r: TodoItem, at: { x: number; y: number }) => {
    const t = r.todo;
    void showContextMenu(menus.todoPanelMenu(t, { report, edit: todoEdit, remove: () => deleteTodos([t]) }), at).catch((e: unknown) => setToast(String(e)));
  };

  /** Link…: the project's worktrees (pinned first) and recent sessions a todo isn't linked to. */
  const todoLinkMenu = (r: TodoItem, at: { x: number; y: number }) => {
    const t = r.todo;
    const inProjectLanes = lanes.filter((l) => !l.hidden && (t.project ? l.project === t.project : inProject(l, current)));
    const pinnedFirst = [...lanePins.keys.flatMap((id) => inProjectLanes.filter((l) => l.id === id)), ...inProjectLanes.filter((l) => !lanePins.set.has(l.id)).sort((a, b) => b.lastActivity - a.lastActivity)];
    const worktreeItems = pinnedFirst
      .filter((l) => !t.worktrees.includes(l.id))
      .slice(0, 20)
      .map((l) => ({
        label: l.branch && l.branch !== l.callsign ? `${l.callsign} · ${l.branch}` : l.callsign,
        run: () => runTodo({ op: 'link', id: t.id, worktree: l.id, linked: true }),
      }));
    const sessionItems = inProjectLanes
      .flatMap((lane) => lane.sessions.filter((s) => !s.visitor && !s.archived && !t.sessions.includes(s.key)).map((session) => ({ lane, session })))
      .sort((a, b) => b.session.lastActivity - a.session.lastActivity)
      .slice(0, 20)
      .map(({ lane, session }) => ({
        label: `${session.account}: ${session.title.slice(0, 56)} (${lane.callsign})`,
        run: () => runTodo({ op: 'link', id: t.id, session: session.key, linked: true }),
      }));
    void showContextMenu(
      [
        { label: 'Worktree', items: worktreeItems },
        { label: 'Session', items: sessionItems },
      ],
      at,
    ).catch((e: unknown) => setToast(String(e)));
  };

  /**
   * "Add to todo" for a session's or a worktree's menu: the project's open todos it isn't on,
   * newest first, and a new one; then taking it off the todos it's on.
   */
  const todoMenuItems = (project: string, link: { worktree?: string; session?: string }, context: ComposerContext, title = ''): MenuSpec => {
    const on = todoStore.todos.filter((t) => (link.session ? t.sessions.includes(link.session) : t.worktrees.includes(link.worktree ?? '')));
    const open = todoStore.todos
      .filter((t) => t.status !== 'done' && !on.includes(t) && (!t.project || t.project === project))
      .sort((a, b) => b.updatedAt - a.updatedAt)
      .slice(0, 15);
    return [
      {
        label: 'Add to todo',
        items: [
          ...open.map((t) => ({ label: t.title || 'Untitled', run: () => runTodo({ op: 'link', id: t.id, ...link, linked: true }) })),
          ...(open.length > 0 ? ['separator' as const] : []),
          { label: 'New todo…', run: () => newTodoFrom(context, title) },
        ],
      },
      ...on.map((t) => ({ label: `Remove from todo “${(t.title || 'Untitled').slice(0, 40)}”`, run: () => runTodo({ op: 'link', id: t.id, ...link, linked: false }) })),
      // A session's own: the todos it's on that aren't done, it as their primary session or not.
      ...(link.session ? on.filter((t) => t.status !== 'done') : []).map((t) => {
        const primary = t.primary?.session === link.session;
        return {
          label: `Primary session for “${(t.title || 'Untitled').slice(0, 40)}”`,
          checked: primary,
          run: () => runTodo({ op: 'primary', id: t.id, session: primary ? null : (link.session ?? null) }),
        };
      }),
    ];
  };


  // The Todo view's own keys, bound fresh each render so they see the rows as they are. N adds a
  // todo, X marks one done (or reopens it), W moves it to Waiting or out, D picks its due date,
  // Delete deletes it, ⌃⌘↑ ⌃⌘↓ move it. ⇧↑ ⇧↓ pick the rows above or below too; then X, W and Delete act on all of
  // them, Return starts one session for them, and Escape goes back to one.
  const todoKeys = useLatest((e: KeyboardEvent, key: string): boolean => {
    if (view !== 'todo') return false;
    // Reordering moves one todo at a time.
    if (e.ctrlKey && e.metaKey && !e.altKey && !e.shiftKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      e.preventDefault();
      if (selected?.kind !== 'todo' || pickedRows.length > 0 || selected.todo.status === 'done') return true;
      const section = todoSecs.find((s) => s.rows.some((r) => r.key === selected.key));
      const i = section?.rows.findIndex((r) => r.key === selected.key) ?? -1;
      // Insertion points: one up is the row's own index less one; one down is two past it.
      if (section && i >= 0) moveTodo(section.id, selected.todo.id, e.key === 'ArrowUp' ? i - 1 : i + 2);
      return true;
    }
    if (e.metaKey || e.ctrlKey) return false;
    if (key === 'n' && !e.altKey) {
      e.preventDefault();
      setComposerFocus((n) => n + 1);
      return true;
    }
    if (selected?.kind !== 'todo') return false;
    const t = selected.todo;
    const ts = pickedRows.length > 0 ? pickedRows.map((r) => r.todo) : [t];
    if (e.shiftKey && !e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      e.preventDefault();
      const i = visible.indexOf(selected);
      const to = visible[Math.min(visible.length - 1, Math.max(0, i + (e.key === 'ArrowUp' ? -1 : 1)))];
      if (to?.kind === 'todo') selectTodoRow(to, 'range');
      return true;
    }
    if (e.key === 'Escape' && pickedRows.length > 0) {
      clearPick();
      return true;
    }
    if (e.key === 'Enter' && !e.altKey && pickedRows.length > 0) {
      e.preventDefault();
      startForTodos(pickedRows);
      return true;
    }
    if (key === 'x' && !e.altKey) {
      // Done, unless they all are: then each goes back to where it was.
      const reopen = ts.every((x) => x.status === 'done');
      for (const x of ts) if (reopen || x.status !== 'done') setTodoStatus(x, !reopen ? 'done' : x.sessions.length > 0 ? 'in-progress' : 'todo');
      return true;
    }
    if (key === 'w' && !e.altKey) {
      toggleWaiting(ts);
      return true;
    }
    if (key === 'd' && !e.altKey && pickedRows.length === 0) {
      e.preventDefault();
      setDuePick(t.id);
      return true;
    }
    // Only from the list: a button in the panel keeps the keyboard's other uses.
    const onList = !document.activeElement || document.activeElement === document.body || !!document.activeElement.closest('.ca-board-list');
    if ((e.key === 'Backspace' || e.key === 'Delete') && onList) {
      e.preventDefault();
      deleteTodos(ts);
      return true;
    }
    return false;
  });

  return {
    duePick,
    duePicked,
    pickedRows,
    pickedKeys,
    composer,
    setComposer,
    composerRef,
    createTodo,
    selectTodoRow,
    showTodo,
    startForTodo,
    startForTodos,
    projectsForTodos,
    todoMenu,
    todoMenuSpec,
    pickMenu,
    todoMoreMenu,
    todoLinkMenu,
    labelMenu,
    moveTodo,
    todoMenuItems,
    todoKeys,
  };
}
