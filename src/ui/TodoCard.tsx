// A todo that waits on you, as a card: in the notch under its pills (NotchTodo.tsx), where the
// queue reaches the todos due today or before and, once nothing else waits, the ones ready to
// start (bar.ts `notchRows`). It's the strip every card has (SessionPanel.tsx `QueueStrip`), with
// what's commonly done to a todo in place of a session's: Start (⌘⏎), Done (⇧⌘E), Snooze (⌃⌘1 to
// ⌃⌘7, out of what waits until then, its due date as it is) and Due…, each a button that says its
// key; then the todo as the main window's panel shows it (TodoPanel.tsx), its changes saved as
// you go through the window's store (todos.ts `useTodoStore`), which shows them at once; then a
// box that starts a session on it, on the account the agent menu picks (Auto as the quick panel
// has it, useStartChoice.ts), in the worktree it's linked to, told the todo's prompt and what
// you write. Done or snoozed, the todo leaves what waits on you at once, in every window
// (ahead.ts `todoLeftAhead`), and comes back with why if that fails; started, it leaves once the
// start went, and a start that fails keeps the card and what you wrote.
//
// A todo linked to no worktree has nowhere to start from here: its box says so, and Start (the
// strip's, the panel's, ⌘⏎) opens the quick panel on it, which asks where.

import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type RefObject } from 'react';
import { todoLeftAhead } from '../ahead';
import { showQuick, startSession, todoOp } from '../api';
import { useLatest } from '../latest';
import { sessionHomes } from '../lanes';
import * as menus from '../menus';
import { chordPressed, cmd, held, keysSaid } from '../platform';
import { errorText, type Report } from '../report';
import { snoozeChorded, todoSnoozeMenu } from '../snooze';
import { AUTO, agentId, autoGroupIn } from '../start';
import { useStartOptions } from '../startAccounts';
import { primaryHolds, todoItem, todosPrompt, type TodoOp } from '../todos';
import type { Label, Lane, Session, Todo, Usage } from '../types';
import { useStartChoice, useStartPicks } from '../useStartChoice';
import { cardZoom } from '../zoom';
import { AgentMenu } from './AgentMenu';
import { reloadKey, zoomKey } from './cardHost';
import { dialogUp } from './focusTrap';
import { BoxText, Button } from './primitives';
import { QueueStrip, snoozeKeys, snoozeRange, type Hotkeys, type Notice, type Queue, type StripAction } from './SessionPanel';
import { TodoPanel } from './TodoPanel';

/** What the window that shows the card does for it. */
export interface TodoCardCan {
  report: Report;
  /** The todo left what waits on you, and how: the window goes on to the next. */
  left: (done: menus.Left) => void;
  /** What took it off didn't go: it's back among what waits, and the window says why. */
  failed: (why: string) => void;
  /** Shows it, or a session or worktree of it, in the main window, and the card goes away. */
  showTodo: (todo: Todo) => void;
  showSession: (lane: Lane, session: Session) => void;
  showLane: (lane: Lane) => void;
  /** Opens a session where it runs. */
  openSession: (lane: Lane, session: Session) => void;
  /** A right-click on a worktree or session it's linked to, or on the todo's header: that one's
   *  menu (menus.ts). */
  laneMenu: (lane: Lane) => void;
  sessionMenu: (lane: Lane, session: Session) => void;
  todoMenu: () => void;
}

// The todos a start is on its way for, whichever card asked: its card can be left and come back
// to before the start answers, and a second start, or a Done the start would then undo (todos.rs
// `started` sets it in progress), mustn't go meanwhile.
const launching = new Set<string>();
const heard = new Set<() => void>();
function setLaunching(id: string, on: boolean): void {
  if (on) launching.add(id);
  else launching.delete(id);
  heard.forEach((f) => f());
}
function useLaunching(id: string): boolean {
  return useSyncExternalStore(
    (f) => {
      heard.add(f);
      return () => heard.delete(f);
    },
    () => launching.has(id),
  );
}

export interface TodoCardProps {
  todo: Todo;
  /** Every todo and label, with the window's changes not yet on the board (todos.ts
   *  `useTodoStore`), and how a change goes. */
  todos: Todo[];
  labels: Label[];
  run: (op: TodoOp) => void;
  lanes: Lane[];
  usage: Usage[];
  projects: string[];
  now: number;
  /** It draws at the zoom, in percent, as a session's card does (zoom.ts `cardZoom`). */
  zoom?: number;
  can: TodoCardCan;
  queue?: Queue;
  notice?: Notice | null;
  hotkeys: Hotkeys;
}

export function TodoCard({ todo, todos, labels, run, lanes, usage, projects, now, zoom, can, queue, notice, hotkeys }: TodoCardProps) {
  const { report } = can;
  const laneById = useMemo(() => new Map(lanes.map((l) => [l.id, l])), [lanes]);
  const homes = useMemo(() => sessionHomes(lanes), [lanes]);
  const labelById = useMemo(() => new Map(labels.map((l) => [l.id, l])), [labels]);
  const item = useMemo(() => todoItem(todo, laneById, homes, labelById), [todo, laneById, homes, labelById]);
  const box = useRef<HTMLTextAreaElement>(null);

  const sending = useLaunching(todo.id);
  // What takes it off what waits on you goes on at once, everywhere; one that fails comes back.
  const leaving = (done: menus.Left, act: Promise<number | null>) => {
    todoLeftAhead([todo.id], act).catch((e: unknown) => can.failed(errorText(e)));
    can.left(done);
  };
  const done = () => {
    if (!launching.has(todo.id)) leaving('done', todoOp({ op: 'update', id: todo.id, patch: { status: 'done' } }));
  };
  // Done from the panel's status, or archiving it from its menu, is the strip's Done: it leaves
  // what waits on you and the card goes on. The rest of its changes go through the store.
  const runHere = (op: TodoOp) => {
    if (op.op === 'update' && op.id === todo.id && (op.patch.status === 'done' || op.patch.archived === true) && Object.keys(op.patch).length === 1) {
      if (op.patch.status === 'done') done();
      else leaving('archived', todoOp(op));
      return;
    }
    run(op);
  };
  const edit: menus.TodoEdit = { run: runHere, todos, labels, projects };
  const snooze = todoSnoozeMenu(todo, now, (act) => leaving('snoozed', act.then(() => null)), true);
  const due = menus.dueItem([todo], edit);
  const starting = useStartHere(item.lane, todo, homes, usage, now, lanes, report, sending, () => leaving('started', Promise.resolve(null)));
  // Start, from the strip, the panel and ⌘⏎: from the box in its worktree, else the quick panel.
  const start = () => (item.lane ? starting.start() : void showQuick({ todo: todo.id, project: todo.project || null }).catch((e: unknown) => report.error(errorText(e))));

  // Done, the snooze times and ⌘⏎ are the card's keys while it shows, as a PR's are its strip's,
  // ⌘⏎ the box's too: one handler, so it's the same wherever the cursor is. ⌘⏎ in a field of the
  // panel (its title, its description) is the field's when it takes it. The zoom's keys and ⌘R
  // are every card's (cardHost.ts).
  const latest = useLatest({ done, snooze, start, report });
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing || dialogUp()) return;
      if (zoomKey(e, latest.current.report) || reloadKey(e, latest.current.report)) {
        e.preventDefault();
        return;
      }
      if (chordPressed(menus.CHORDS.done ?? '', e)) {
        e.preventDefault();
        latest.current.done();
        return;
      }
      if (e.key === 'Enter' && cmd(e) && !e.shiftKey && !e.altKey) {
        e.preventDefault();
        latest.current.start();
        return;
      }
      const time = snoozeChorded(latest.current.snooze, e.key);
      if (time && held(e, 'Ctrl', 'Cmd')) {
        e.preventDefault();
        time.run();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [latest]);

  const buttons: StripAction[] = [
    { label: 'Start', keys: keysSaid('⌘⏎'), title: sending ? 'A session is starting on it' : item.lane ? `Start a session on it in ${item.lane.callsign}, with what’s written in the box at its foot` : 'It’s linked to no worktree: the quick panel asks where to start it', run: sending ? undefined : start },
    { label: 'Done', chord: menus.CHORDS.done, title: sending ? 'A session is starting on it: it’s marked done once that’s settled' : 'Mark it done: it leaves what waits on you', run: todo.status === 'done' || sending ? undefined : done },
    { label: 'Snooze', keys: snoozeRange(), title: `Snooze it: it waits apart until its time comes, its due date as it is. ${snoozeKeys(snooze)}`, menu: snooze.items },
    { label: 'Due…', title: 'When it’s due', menu: due.items },
  ];

  return (
    <div className="ca-reply ca-todocard" style={cardZoom(zoom)}>
      <QueueStrip queue={queue} notice={notice} hotkeys={hotkeys} buttons={buttons} />
      <div className="ca-todocard-body">
        <TodoPanel
          item={item}
          todos={todos}
          labels={labels}
          lanes={laneById}
          now={now}
          projects={projects}
          onRun={runHere}
          onStart={start}
          onMenu={can.todoMenu}
          onShowTodo={can.showTodo}
          onShowSession={(lane, session) => can.showSession(lane, session)}
          onOpenSession={can.openSession}
          onShowLane={can.showLane}
          onLaneMenu={(lane) => can.laneMenu(lane)}
          onSessionMenu={can.sessionMenu}
          // Linking, moving and deleting are the main window's, where they can be undone.
          onLinkMenu={() => can.showTodo(todo)}
          onMoreMenu={() => can.showTodo(todo)}
          onLabelMenu={() => can.showTodo(todo)}
          onError={report.error}
        />
      </div>
      <TodoStartBox boxRef={box} lane={item.lane} starting={starting} />
    </div>
  );
}

/** Starting a session on the todo in `lane` from the card's box: the account and place the agent
 *  menu's pick comes to (useStartChoice.ts), the todo's prompt with what's written after it. The
 *  card goes on (`onStarted`) once the start went; one that fails says why and keeps what's
 *  written. What the start says you're to do next (send the prompt the Claude app holds) is said
 *  on the bar. */
function useStartHere(lane: Lane | null, todo: Todo, homes: ReturnType<typeof sessionHomes>, usage: Usage[], now: number, lanes: Lane[], report: Report, sending: boolean, onStarted: () => void) {
  const options = useStartOptions(!!lane);
  const picks = useStartPicks();
  const chosen = useStartChoice(picks, options, usage, now, lanes, lane?.project ?? null);
  const autoName = autoGroupIn(options, lane?.project ?? null);
  const autoGroup = chosen.groups.find((g) => g.name === autoName) ?? null;
  const [written, setWritten] = useState('');
  const start = () => {
    if (!lane || launching.has(todo.id)) return;
    const { account, placeNow } = chosen;
    if (!account || !placeNow) {
      report.error('No account can start a session here now.');
      return;
    }
    const prompt = [todosPrompt([todo], (t) => primaryHolds(t, homes, now)), written.trim()].filter(Boolean).join('\n\n');
    const id = todo.id;
    setLaunching(id, true);
    startSession({ laneId: lane.id, account, place: placeNow, prompt, model: chosen.modelNow, effort: chosen.effortNow ?? null, worktree: chosen.inNewWorktree, todos: [todo.id] })
      .then(
        (started) => {
          if (started?.message) report.info(started.message);
          onStarted();
        },
        (e: unknown) => report.error(errorText(e)),
      )
      .finally(() => setLaunching(id, false));
  };
  const pick = (p: { account: string; model: string | null; effort: string | null }) => {
    picks.setAgent(agentId(p.account, p.model, chosen.groups));
    picks.setEffort(p.effort);
  };
  return { chosen, effort: picks.effort, autoGroup, written, setWritten, sending, start, pick, ready: !!options, usage, now, lanes };
}

/** The box at the card's foot: what's added to the todo's prompt, and the agent menu. ⌘⏎ starts. */
function TodoStartBox({ boxRef, lane, starting: s }: { boxRef: RefObject<HTMLTextAreaElement | null>; lane: Lane | null; starting: ReturnType<typeof useStartHere> }) {
  const { chosen } = s;
  if (!lane) {
    return <p className="ca-todocard-nowhere">It’s linked to no worktree, so it can’t start from here: Start opens the quick panel, which asks where.</p>;
  }
  return (
    <div className="ca-composer ca-todocard-box">
      <div
        className="ca-composer-field"
        onMouseDown={(e) => {
          if ((e.target as Element).closest('button, textarea, .ca-picker')) return;
          e.preventDefault();
          boxRef.current?.focus();
        }}
      >
        <BoxText ref={boxRef} value={s.written} lines={6} hint={`Anything to add? ${keysSaid('⌘⏎')} starts a session on it in ${lane.callsign}`} aria-label="What the session is told after the todo" onChange={(e) => s.setWritten(e.target.value)} />
        <div className="ca-composer-buttons">
          <AgentMenu
            accounts={chosen.accounts}
            usage={s.usage}
            now={s.now}
            value={{ account: chosen.choice ?? AUTO, model: chosen.chosenModel, effort: s.effort }}
            auto={{ title: 'The account Auto picks, as the quick panel does', groups: chosen.groups, group: s.autoGroup, lanes: s.lanes }}
            title="Which account it starts on, on which model and at which effort"
            disabled={!s.ready || chosen.accounts.length === 0}
            onPick={(p) => s.pick({ account: p.account, model: p.model ?? null, effort: p.effort ?? null })}
            onDone={() => boxRef.current?.focus()}
          />
          <Button size="sm" variant="primary" disabled={!s.ready || s.sending} title={`Start a session on it in ${lane.callsign} (${keysSaid('⌘⏎')})`} onClick={s.start}>
            {s.sending ? 'Starting…' : 'Start'}
          </Button>
        </div>
      </div>
    </div>
  );
}
