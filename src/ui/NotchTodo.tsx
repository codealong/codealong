// A todo that waits on you, under the pills of the bar docked in the notch (Notch.tsx), in the
// place a session takes (NotchSession.tsx): a click on the due or ready pill, or on the due count
// beside the camera, opens the notch into the todo's card (TodoCard.tsx), dark as the notch is. It
// works through what waits on you with the same keys as a session there, from the same code
// (queue.ts, useWaitingQueue.ts), the todos among the sessions and reviews in the bar's order
// (bar.ts `notchRows`): ⇧⌘E marks it done and goes on, ⌃⌘1 to ⌃⌘7 snooze it and go on, ⌘⏎
// starts a session on it and goes on, ⌘] skips. The card follows the board while it's up, for the
// todo and what's around it, with the changes made here shown at once (todos.ts `useTodoStore`):
// the pill carries only its id and title.

import { useCallback, useEffect, useRef, useState } from 'react';
import { showQuick } from '../api';
import { showContextMenu } from '../contextMenu';
import { useEditorIcons } from '../editors';
import { followBoard, type Following } from '../followBoard';
import * as menus from '../menus';
import { errorText } from '../report';
import { useTodoStore } from '../todos';
import type { Board, Lane, Session } from '../types';
import { useCardHost } from './cardHost';
import type { Notice, Queue } from './SessionPanel';
import { TodoCard } from './TodoCard';

interface NotchTodoProps {
  /** The todo to show, by its row's key (bar.ts `todoRowKey`). */
  todoKey: string;
  now: number;
  queue: Queue | undefined;
  notice: Notice | null;
  /** Back to the one looked at before (⌃-); null with none. */
  onBack: (() => void) | null;
  /** It has the keyboard; a click in it takes it. */
  keys: boolean;
  onKeys: () => void;
  onClose: () => void;
  /** The todo left what waits on you, and how: on to the next. */
  onLeft: (key: string, done: menus.Left) => void;
  /** What took it off didn't go: back among what waits, with why. */
  onFailed: (key: string, why: string) => void;
  /** The board no longer has it. */
  onGone: (key: string) => void;
  /** The zoom the card draws at, in percent, from the bar's feed (zoom.ts). */
  zoom?: number;
}

export function NotchTodo({ todoKey, now, queue, notice, onBack, keys, onKeys, onClose, onLeft, onFailed, onGone, zoom }: NotchTodoProps) {
  const id = todoKey.slice('todo:'.length);
  const [board, setBoard] = useState<Board | null>(null);
  const following = useRef<Following | null>(null);
  useEffect(() => {
    const f = followBoard((b) => setBoard(b));
    following.current = f;
    return () => {
      f.off();
      following.current = null;
    };
  }, []);
  const needBoard = useCallback(() => following.current?.read(), []);

  const apps = board?.apps;
  const host = useCardHost({ sessionKey: null, anchor: 'due', live: { board, needBoard, there: undefined }, apps, terminal: apps?.terminal ?? 'Terminal', away: onClose });
  const icons = useEditorIcons(apps);
  const onDesktop = menus.useDesktopPins();
  const { report } = host;
  const store = useTodoStore(board, report.error);
  const todo = store.todos.find((t) => t.id === id) ?? null;
  const gone = useRef(onGone);
  gone.current = onGone;
  useEffect(() => {
    if (board && !todo) gone.current(todoKey);
  }, [board, todo, todoKey]);
  if (!board || !todo) return null;

  const show = (spec: Parameters<typeof showContextMenu>[0]) => void showContextMenu(spec).catch((e: unknown) => report.error(errorText(e)));
  const editors = apps ? { apps, icons } : undefined;
  const sessionCan = (l: Lane, s: Session): menus.SessionCan => ({ report, open: host.open, terminal: apps?.terminal ?? 'Terminal', reveal: () => host.showSession(l, s), editors, desktop: onDesktop });

  return (
    <div
      className="ca-notch-card"
      onPointerDown={() => {
        if (!keys) onKeys();
      }}
    >
      <TodoCard
        key={todo.id}
        todo={todo}
        todos={store.todos}
        labels={store.labels}
        run={(op) => void store.run(op)}
        lanes={board.lanes}
        usage={board.usage}
        projects={board.projects.map((p) => p.name)}
        now={now}
        zoom={zoom}
        can={{
          report,
          left: (done) => onLeft(todoKey, done),
          failed: (why) => onFailed(todoKey, why),
          showTodo: host.showTodo,
          showSession: host.showSession,
          showLane: host.showLane,
          openSession: (lane, session) => host.open(lane, session.openTarget),
          laneMenu: (l) => show(menus.laneMenu(l, { report, open: host.open, reveal: () => host.showLane(l), editors, desktop: onDesktop })),
          sessionMenu: (l, s) => show(menus.sessionMenu(l, s, sessionCan(l, s))),
          todoMenu: () =>
            show(
              menus.todoMenu(todo, {
                report,
                start: () => void showQuick({ todo: todo.id, project: todo.project || null }).catch((e: unknown) => report.error(errorText(e))),
                reveal: () => host.showTodo(todo),
                edit: { run: (op) => void store.run(op), todos: store.todos, labels: store.labels, projects: board.projects.map((p) => p.name) },
                desktop: onDesktop,
              }),
            ),
        }}
        queue={queue}
        notice={notice}
        hotkeys={{ onBack, onClose }}
      />
    </div>
  );
}
