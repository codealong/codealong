// The panel on the right of the main window, as the view and its selection have it: a
// worktree's (Inspector), a session's with its conversation (SessionPanel) or the sessions picked
// together, a PR's, a todo's or the todos picked together, the canvas's selected card, or what to
// select. A PR's or todo's
// panel can show one of its sessions in its place, with the way back (`drill`). The window
// passes what the panels show and what their clicks do.

import { dismissTodoHint, openUrl } from '../api';
import type { Entities } from '../canvas/model';
import { showContextMenu, type MenuSpec } from '../contextMenu';
import { deviceOf } from '../devices';
import { middleTruncate } from '../format';
import { sessionPrs, type Placed } from '../lanes';
import { lineageOf } from '../lineage';
import type * as menus from '../menus';
import type { ListRow } from '../boardLists';
import type { TodoItem, useTodoStore } from '../todos';
import type { Board, DevicesView, Lane, Pr, PrItem, Session, Todo } from '../types';
import type { WaitingQueue } from '../useWaitingQueue';
import type { useListPicks } from '../useListPicks';
import type { useTodoView } from '../useTodoView';
import { Inspector } from './Inspector';
import { PrInspector } from './PrInspector';
import { ALL_PROJECTS } from './ProjectSwitcher';
import type { PrRowItem } from './PrsView';
import { SessionPanel } from './SessionPanel';
import { PrPickPanel } from './PrPickPanel';
import { SessionPickPanel } from './SessionPickPanel';
import { WorktreePickPanel } from './WorktreePickPanel';
import { TodoPanel, TodoPickPanel } from './TodoPanel';

/** What the canvas's selected card is, as its panel shows it; null when nothing is selected or
 *  the card's thing is gone. */
export type CanvasPicked = { kind: 'lane'; lane: Lane } | { kind: 'session'; lane: Lane; session: Session } | { kind: 'todo'; item: TodoItem } | { kind: 'pr'; pr: PrItem } | null;

export function canvasPicked(selected: { ref: 'worktree' | 'session' | 'todo' | 'pr'; key: string } | null, entities: Entities, todoItems: TodoItem[]): CanvasPicked {
  if (!selected) return null;
  switch (selected.ref) {
    case 'worktree': {
      const lane = entities.lanes.get(selected.key);
      return lane ? { kind: 'lane', lane } : null;
    }
    case 'session': {
      const p = entities.sessions.get(selected.key);
      return p ? { kind: 'session', lane: p.lane, session: p.session } : null;
    }
    case 'todo': {
      const item = todoItems.find((r) => r.todo.id === selected.key);
      return item ? { kind: 'todo', item } : null;
    }
    case 'pr': {
      const pr = entities.prs.get(selected.key);
      return pr ? { kind: 'pr', pr } : null;
    }
  }
}

type TodoView = ReturnType<typeof useTodoView>;

export interface MainPanelProps {
  view: string;
  selected: ListRow | null;
  selectedLane: Lane | null;
  /** The canvas's selected card, on the canvas. */
  canvasPicked: CanvasPicked;
  board: Board | null;
  now: number;
  loading: boolean;
  lanes: Lane[];
  laneById: Map<string, Lane>;
  homes: Map<string, Placed>;
  devices: DevicesView | null;
  current: string;
  projectTodos: Todo[];
  todoStore: Pick<ReturnType<typeof useTodoStore>, 'todos' | 'labels' | 'run'>;
  todoLinks: { bySession: Map<string, Todo[]>; byLane: Map<string, Todo[]> };
  todosFor: (key: string) => Todo[] | undefined;
  prRowOf: (pr: PrItem) => PrRowItem;
  pickedRows: TodoItem[];
  todo: Pick<TodoView, 'startForTodo' | 'startForTodos' | 'todoMenu' | 'todoLinkMenu' | 'todoMoreMenu' | 'labelMenu' | 'projectsForTodos' | 'duePick' | 'duePicked' | 'selectTodoRow' | 'pickMenu'>;
  /** Rows picked together in Sessions, on the Worktrees board and in PRs (useListPicks.ts):
   *  their panel shows in place of one row's. */
  picks: ReturnType<typeof useListPicks>;
  approving: Set<string>;
  /** The session a PR's or todo's panel shows in its place, and from which. */
  drill: { from: string; key: string } | null;
  setDrill: (drill: { from: string; key: string } | null) => void;
  /** Sessions' way through what waits on you, which its panel works through. */
  queue: WaitingQueue<boolean>;
  /** The row whose panel's box takes the cursor. */
  boxFocus: string | null;
  panelMax: boolean;
  togglePanelMax: () => void;
  actionsRef: { readonly current: { session: (lane: Lane, s: Session, pin: 'session' | 'worktree') => menus.SessionActions } };
  open: (lane: Lane | null, targetId: string | null | undefined) => void;
  newSession: (at: { laneId: string | null }) => void;
  showLaneFromPanel: (lane: Lane) => void;
  showSessionFromPanel: (lane: Lane, session: Session) => void;
  showTodoFromPanel: (todo: Todo) => void;
  laneMenu: (lane: Lane) => void;
  sessionMenu: (lane: Lane, s: Session, pin: 'session' | 'worktree') => void;
  sessionMenuSpec: (lane: Lane, s: Session, pin: 'session' | 'worktree') => MenuSpec;
  lanePrMenu: (lane: Lane) => void;
  /** A PR's right-click menu, as its row in PRs has it. */
  prMenu: (r: PrRowItem) => void;
  prChipMenu: (lane: Lane, pr: Pr) => void;
  approve: (pr: PrItem) => void;
  reviewMenuFrom: (pr: Pick<PrItem, 'number' | 'title' | 'url' | 'repoSlug' | 'lanes'>, el: Element) => void;
  /** Drops Snooze review's times from a button. */
  snoozeMenuFrom: (pr: PrItem, el: Element) => void;
  reviewedPr: (lane: Lane, pr: Pr) => Pick<PrItem, 'number' | 'title' | 'url' | 'repoSlug' | 'lanes'>;
  setPreviewFor: (laneId: string) => void;
  markSeen: (keys: string[]) => void;
  setToast: (message: string | null) => void;
}

export function MainPanel(props: MainPanelProps) {
  const { view, selected, selectedLane, canvasPicked, board, now, loading, lanes, laneById, homes, devices, current, projectTodos, todoStore, todoLinks, todosFor, prRowOf, pickedRows, picks, approving, drill, setDrill, queue, boxFocus, panelMax, togglePanelMax, actionsRef, open, newSession, showLaneFromPanel, showSessionFromPanel, showTodoFromPanel, laneMenu, sessionMenu, sessionMenuSpec, lanePrMenu, prMenu, prChipMenu, approve, reviewMenuFrom, snoozeMenuFrom, reviewedPr, setPreviewFor, markSeen, setToast } = props;
  const { startForTodo, startForTodos, todoMenu, todoLinkMenu, todoMoreMenu, labelMenu, projectsForTodos, duePick, duePicked, selectTodoRow, pickMenu } = props.todo;
  const runTodo = todoStore.run;
  const { run: runQueue } = queue;

  const todoPanel = (item: TodoItem) =>
    drilledPanel(item.key, middleTruncate(item.todo.title, 40)) ?? (
    <TodoPanel
      key={item.todo.id}
      item={item}
      todos={todoStore.todos}
      onShowTodo={showTodoFromPanel}
      labels={todoStore.labels}
      lanes={laneById}
      now={now}
      onRun={runTodo}
      onStart={() => startForTodo(item)}
      onShowSession={(_lane, session) => setDrill({ from: item.key, key: session.key })}
      onOpenSession={(lane, session) => open(lane, session.openTarget)}
      onShowLane={showLaneFromPanel}
      onLaneMenu={(lane) => laneMenu(lane)}
      onSessionMenu={(lane, s) => sessionMenu(lane, s, 'worktree')}
      onLinkMenu={(at) => todoLinkMenu(item, at)}
      onMoreMenu={(at) => todoMoreMenu(item, at)}
      onMenu={() => todoMenu(item)}
      onLabelMenu={labelMenu}
      onError={setToast}
      hint={board?.jev?.todos ? board.todoHints?.[item.todo.id] : null}
      onDismissHint={() => void dismissTodoHint(item.todo.id)}
      suggest={!!board?.jev?.todos}
      projects={projectsForTodos}
      pickDue={duePick === item.todo.id}
      onPickDue={duePicked}
    />
  );
  /** Several todos picked together: one session for all of them. */
  const todoPickPanel = (rows: TodoItem[]) => (
    <TodoPickPanel
      items={rows}
      todos={todoStore.todos}
      showProject={current === ALL_PROJECTS}
      onStart={() => startForTodos(rows)}
      onShow={(r) => selectTodoRow(r, 'one')}
      onLeaveOut={(r) => selectTodoRow(r, 'toggle')}
      onMoreMenu={(at) => pickMenu(rows, at)}
    />
  );
  const prPanel = (item: PrRowItem) =>
    drilledPanel(item.key, `PR #${item.pr.number}`) ?? (
    <PrInspector
      key={item.pr.key}
      item={item}
      now={now}
      approving={approving.has(item.pr.key)}
      onApprove={() => approve(item.pr)}
      report={{ error: setToast, info: setToast }}
      usage={board?.usage ?? []}
      onReview={(el) => reviewMenuFrom(item.pr, el)}
      onSnooze={(el) => snoozeMenuFrom(item.pr, el)}
      onMenu={() => prMenu(item)}
      onShowSession={(_lane, session) => setDrill({ from: item.key, key: session.key })}
      onOpenSession={(lane, session) => open(lane, session.openTarget)}
      onShowLane={showLaneFromPanel}
      onLaneMenu={laneMenu}
      onSessionMenu={(lane, s) => sessionMenu(lane, s, 'worktree')}
    />
  );
  const inspector = (lane: Lane | null, focusKey: string | null) => (
    <Inspector
      lane={lane}
      focusKey={focusKey}
      apps={board?.apps ?? { terminal: 'Terminal', terminalId: '', editor: 'Editor', editorId: '' }}
      now={now}
      onOpen={(id) => open(lane, id)}
      onNewSession={() => lane && newSession({ laneId: lane.id })}
      todos={lane ? todoLinks.byLane.get(lane.id) : undefined}
      todosFor={todosFor}
      onShowTodo={showTodoFromPanel}
      onMenu={lane ? () => laneMenu(lane) : undefined}
      onPrMenu={() => lane && lanePrMenu(lane)}
      onReviewPr={lane?.pr ? (el) => lane.pr && reviewMenuFrom(reviewedPr(lane, lane.pr), el) : undefined}
      onSetPreview={lane && !lane.device ? () => setPreviewFor(lane.id) : undefined}
    />
  );

  /** A session's own panel: its last prompt and response, how to open it, its details. Given its
   *  row in Sessions, it works through what waits on you from there. */
  const sessionPanel = (lane: Lane, session: Session, back?: { label: string; onBack: () => void }, row?: string) => {
    // In Sessions it works through what waits on you: what's left after it, and how the last went.
    const queued = row !== undefined;
    return (
      <SessionPanel
        key={session.key}
        lane={lane}
        session={session}
        apps={board?.apps ?? { terminal: 'Terminal', terminalId: '', editor: 'Editor', editorId: '' }}
        now={now}
        onOpen={(id) => open(lane, id)}
        onShowLane={showLaneFromPanel}
        prs={sessionPrs(board?.prs ?? [], lane, session)}
        onOpenPr={(p) => openUrl(p.url).catch((e: unknown) => setToast(String(e)))}
        onPrMenu={(p) => prChipMenu(lane, p)}
        todos={todoLinks.bySession.get(session.key)}
        onShowTodo={showTodoFromPanel}
        lineage={lineageOf(session, homes)}
        onShowSession={showSessionFromPanel}
        onSessionMenu={(l, x) => sessionMenu(l, x, 'session')}
        onMenu={(at) => void showContextMenu(sessionMenuSpec(lane, session, 'session'), at).catch((err: unknown) => setToast(String(err)))}
        onThreadMenu={(items) => void showContextMenu(items).catch((err: unknown) => setToast(String(err)))}
        // The keyboard stays where it was: the next one's box doesn't take the cursor.
        onSeen={() => (queued ? void runQueue('seen', false) : markSeen([session.key]))}
        device={deviceOf(devices, session.key)}
        back={back}
        onFailed={(why) => {
          // Not dealt with after all: it comes round again.
          queue.again(session.key);
          setToast(`Didn’t send to ${session.title}, so it’s back in its box: ${why}`);
        }}
        queue={queued ? queue.queue : undefined}
        notice={queued ? queue.notice : null}
        // In Sessions the strip is always there, with what's commonly done to a session and the
        // keys for it. Back and forward are in the title bar.
        hotkeys={row !== undefined ? {} : undefined}
        actions={row !== undefined ? actionsRef.current.session(lane, session, 'session') : undefined}
        onDone={queued ? (done) => void queue.answered(session.key, done) : undefined}
        autoFocus={row !== undefined && boxFocus === row}
        maximized={row !== undefined && panelMax}
        onMaximize={row !== undefined ? togglePanelMax : undefined}
      />
    );
  };
  /** The session a PR's or todo's panel was asked to show, in its place, with the way back. */
  const drilledPanel = (from: string, label: string) => {
    if (drill?.from !== from) return null;
    for (const lane of lanes) {
      const session = lane.sessions.find((x) => x.key === drill.key);
      if (session) return sessionPanel(lane, session, { label, onBack: () => setDrill(null) });
    }
    return null;
  };

  /** The canvas's selected card as its view's panel shows it. */
  const canvasPanel = (() => {
    switch (canvasPicked?.kind) {
      case 'lane':
        return inspector(canvasPicked.lane, null);
      case 'session':
        return sessionPanel(canvasPicked.lane, canvasPicked.session);
      case 'todo':
        return todoPanel(canvasPicked.item);
      case 'pr':
        return prPanel(prRowOf(canvasPicked.pr));
      default:
        return null;
    }
  })();

  return view === 'canvas' ? (
    canvasPanel
  ) : view === 'todo' ? (
    pickedRows.length > 0 ? (
      todoPickPanel(pickedRows)
    ) : selected?.kind === 'todo' ? (
      todoPanel(selected)
    ) : (
      <aside className="ca-insp ca-insp--empty">
        <p>{projectTodos.length || loading ? 'Select a todo to see its description, labels and the sessions on it.' : 'Todos are work you mean to hand an agent. Add one on the left.'}</p>
      </aside>
    )
  ) : view === 'prs' ? (
    picks.prs.rows.length > 0 ? (
      <PrPickPanel items={picks.prs.rows} actions={picks.prs.actions()} onShow={(r) => picks.prs.select(r, 'one')} onLeaveOut={(r) => picks.prs.select(r, 'toggle')} onMoreMenu={picks.prs.menu} />
    ) : selected?.kind === 'pr' ? (
      prPanel(selected)
    ) : (
      <aside className="ca-insp ca-insp--empty">
        <p>Select a PR to see its description, reviews and changes.</p>
      </aside>
    )
  ) : view === 'sessions' && picks.sessions.rows.length > 0 ? (
    <SessionPickPanel
      items={picks.sessions.rows}
      actions={picks.sessions.actions()}
      onShow={(r) => picks.sessions.select(r, 'one')}
      onLeaveOut={(r) => picks.sessions.select(r, 'toggle')}
      onMoreMenu={picks.sessions.menu}
    />
  ) : view === 'worktrees' && picks.worktrees.rows.length > 0 ? (
    <WorktreePickPanel
      items={picks.worktrees.rows}
      actions={picks.worktrees.actions()}
      onShow={(r) => picks.worktrees.select(r, 'one')}
      onLeaveOut={(r) => picks.worktrees.select(r, 'toggle')}
      onMoreMenu={picks.worktrees.menu}
    />
  ) : view === 'sessions' && selected?.kind === 'agent' ? (
    sessionPanel(selected.lane, selected.session, undefined, selected.key)
  ) : (
    inspector(selectedLane, selected?.kind === 'agent' ? selected.session.key : null)
  );
}
