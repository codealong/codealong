// A todo that waits on you, as a card: in the notch under its pills (NotchTodo.tsx), where the
// queue reaches the todos due today or before and, once nothing else waits, the ones ready to
// start (bar.ts `notchRows`). It's the strip every card has (SessionPanel.tsx `QueueStrip`), with
// what's commonly done to a todo in place of a session's: Start (⌘⏎, from the box at its foot),
// Done (⇧⌘E), Snooze (⌃⌘1 to ⌃⌘7, out of what waits until then, its due date as it is) and Due…,
// each a button that says its key; then the todo as the main window's panel shows it
// (TodoPanel.tsx), its changes saved as you go; then a box that starts a session on it, on the
// account the agent menu picks (Auto as the quick panel has it, useStartChoice.ts), in the
// worktree it's linked to, told the todo's prompt and what you write. Done, snoozed or started,
// the todo leaves what waits on you, and the window goes on to the next.
//
// A todo linked to no worktree has nowhere to start from here: its box says so, and Start opens
// the quick panel on it, which asks where.

import { useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { showQuick, startOptions, startSession, todoOp } from '../api';
import { useLatest } from '../latest';
import { sessionHomes } from '../lanes';
import * as menus from '../menus';
import { chordPressed, cmd, held, keysSaid } from '../platform';
import { errorText, type Report } from '../report';
import { snoozeChorded, todoSnoozeMenu } from '../snooze';
import { AUTO, agentId } from '../start';
import { todoItem, todosPrompt, type TodoOp } from '../todos';
import type { Label, Lane, Session, StartOptions, Todo, Usage } from '../types';
import { useStartChoice, useStartPicks } from '../useStartChoice';
import { AgentMenu } from './AgentMenu';
import { dialogUp } from './focusTrap';
import { BoxText, Button } from './primitives';
import { QueueStrip, snoozeKeys, snoozeRange, type Hotkeys, type Notice, type Queue, type StripAction } from './SessionPanel';
import { TodoPanel } from './TodoPanel';

/** What the window that shows the card does for it. */
export interface TodoCardCan {
  report: Report;
  /** The todo left what waits on you, and how: the window goes on to the next. */
  left: (done: menus.Left) => void;
  /** Shows it, or a session or worktree of it, in the main window, and the card goes away. */
  showTodo: (todo: Todo) => void;
  showSession: (lane: Lane, session: Session) => void;
  showLane: (lane: Lane) => void;
  /** Opens a session where it runs. */
  openSession: (lane: Lane, session: Session) => void;
}

export interface TodoCardProps {
  todo: Todo;
  /** Every todo and label and worktree, from the board the card follows. */
  todos: Todo[];
  labels: Label[];
  lanes: Lane[];
  usage: Usage[];
  projects: string[];
  now: number;
  can: TodoCardCan;
  queue?: Queue;
  notice?: Notice | null;
  hotkeys: Hotkeys;
}

export function TodoCard({ todo, todos, labels, lanes, usage, projects, now, can, queue, notice, hotkeys }: TodoCardProps) {
  const { report } = can;
  const laneById = useMemo(() => new Map(lanes.map((l) => [l.id, l])), [lanes]);
  const homes = useMemo(() => sessionHomes(lanes), [lanes]);
  const labelById = useMemo(() => new Map(labels.map((l) => [l.id, l])), [labels]);
  const item = useMemo(() => todoItem(todo, laneById, homes, labelById), [todo, laneById, homes, labelById]);
  const box = useRef<HTMLTextAreaElement>(null);

  const run = (op: TodoOp) => void todoOp(op).catch((e: unknown) => report.error(errorText(e)));
  const edit: menus.TodoEdit = { run, todos, labels, projects };
  // What takes it off what waits on you goes on at once; what goes wrong is said after.
  const leaving = (done: menus.Left, act: Promise<unknown>) => {
    can.left(done);
    act.catch((e: unknown) => report.error(errorText(e)));
  };
  const done = () => leaving('done', todoOp({ op: 'update', id: todo.id, patch: { status: 'done' } }));
  const snooze = todoSnoozeMenu(todo, now, (act) => leaving('snoozed', act), true);
  const due = menus.dueItem([todo], edit);

  // Done and the snooze times are the card's keys while it shows, as a PR's are its strip's.
  const latest = useLatest({ done, snooze });
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing || dialogUp()) return;
      if (chordPressed(menus.CHORDS.done ?? '', e)) {
        e.preventDefault();
        latest.current.done();
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

  const startHere = useStartHere(item.lane, todo, todos, usage, now, lanes, report, (act) => leaving('started', act));
  const buttons: StripAction[] = [
    { label: 'Start', keys: keysSaid('⌘⏎'), title: item.lane ? `Start a session on it in ${item.lane.callsign}, with what’s written in the box at its foot` : 'It’s linked to no worktree: the quick panel asks where to start it', run: () => (item.lane ? startHere.start() : void showQuick({ todo: todo.id, project: todo.project || null }).catch((e: unknown) => report.error(errorText(e)))) },
    { label: 'Done', chord: menus.CHORDS.done, title: 'Mark it done: it leaves what waits on you', run: todo.status === 'done' ? undefined : done },
    { label: 'Snooze', keys: snoozeRange(), title: `Snooze it: it waits apart until its time comes, its due date as it is. ${snoozeKeys(snooze)}`, menu: snooze.items },
    { label: 'Due…', title: 'When it’s due', menu: due.items },
  ];

  return (
    <div className="ca-reply ca-todocard">
      <QueueStrip queue={queue} notice={notice} hotkeys={hotkeys} buttons={buttons} />
      <div className="ca-todocard-body">
        <TodoPanel
          item={item}
          todos={todos}
          labels={labels}
          lanes={laneById}
          now={now}
          projects={projects}
          onRun={run}
          onStart={() => box.current?.focus()}
          onShowTodo={can.showTodo}
          onShowSession={(lane, session) => can.showSession(lane, session)}
          onOpenSession={can.openSession}
          onShowLane={can.showLane}
          onLaneMenu={(lane) => can.showLane(lane)}
          onSessionMenu={(lane, session) => can.showSession(lane, session)}
          // Linking, moving and deleting are the main window's, where they can be undone.
          onLinkMenu={() => can.showTodo(todo)}
          onMoreMenu={() => can.showTodo(todo)}
          onLabelMenu={() => can.showTodo(todo)}
          onError={report.error}
        />
      </div>
      <TodoStartBox boxRef={box} lane={item.lane} start={startHere} />
    </div>
  );
}

/** Starting a session on the todo in `lane` from the card's box: the account and place the agent
 *  menu's pick comes to (useStartChoice.ts), the todo's prompt with what's written after it. */
function useStartHere(lane: Lane | null, todo: Todo, todos: Todo[], usage: Usage[], now: number, lanes: Lane[], report: Report, onStarted: (act: Promise<unknown>) => void) {
  const [options, setOptions] = useState<StartOptions | null>(null);
  useEffect(() => {
    let over = false;
    startOptions().then(
      (o) => !over && setOptions(o),
      (e: unknown) => !over && report.error(errorText(e)),
    );
    return () => {
      over = true;
    };
  }, [report]);
  const picks = useStartPicks();
  const chosen = useStartChoice(picks, options, usage, now, lanes, lane?.project ?? null);
  const [written, setWritten] = useState('');
  const [sending, setSending] = useState(false);
  const start = () => {
    if (!lane || sending) return;
    const { account, placeNow } = chosen;
    if (!account || !placeNow) {
      report.error('No account can start a session here now.');
      return;
    }
    const prompt = [todosPrompt([todo], (t) => !!t.primary && todos.some((x) => x.id === t.id)), written.trim()].filter(Boolean).join('\n\n');
    setSending(true);
    const act = startSession({ laneId: lane.id, account, place: placeNow, prompt, model: chosen.modelNow, effort: chosen.effortNow ?? null, worktree: chosen.inNewWorktree, todos: [todo.id] }).finally(() => setSending(false));
    onStarted(act);
  };
  const pick = (p: { account: string; model: string | null; effort: string | null }) => {
    picks.setAgent(agentId(p.account, p.model, chosen.groups));
    picks.setEffort(p.effort);
  };
  return { chosen, written, setWritten, sending, start, pick, ready: !!options, usage, now };
}

/** The box at the card's foot: what's added to the todo's prompt, and the agent menu. ⌘⏎ starts. */
function TodoStartBox({ boxRef, lane, start: s }: { boxRef: React.RefObject<HTMLTextAreaElement | null>; lane: Lane | null; start: ReturnType<typeof useStartHere> }) {
  const { chosen } = s;
  const onKeyDown = (e: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && cmd(e)) {
      e.preventDefault();
      s.start();
    }
  };
  if (!lane) {
    return <p className="ca-todocard-nowhere">It’s linked to no worktree, so it can’t start from here: Start opens the quick panel, which asks where.</p>;
  }
  return (
    <div className="ca-composer ca-todocard-box">
      <div className="ca-composer-field" onMouseDown={(e) => !(e.target as Element).closest('button, textarea, .ca-picker') && (e.preventDefault(), boxRef.current?.focus())}>
        <BoxText ref={boxRef} value={s.written} lines={6} hint={`Anything to add? ${keysSaid('⌘⏎')} starts a session on it in ${lane.callsign}`} aria-label="What the session is told after the todo" onChange={(e) => s.setWritten(e.target.value)} onKeyDown={onKeyDown} />
        <div className="ca-composer-buttons">
          <AgentMenu
            accounts={chosen.accounts}
            usage={s.usage}
            now={s.now}
            value={{ account: chosen.choice ?? AUTO, model: chosen.chosenModel, effort: null }}
            auto={{ title: 'The account Auto picks, as the quick panel does', groups: chosen.groups }}
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
