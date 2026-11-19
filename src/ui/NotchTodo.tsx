// A todo that waits on you, under the pills of the bar docked in the notch (Notch.tsx), in the
// place a session takes (NotchSession.tsx): a click on the due or ready pill, or on the due
// count beside the camera, opens the notch into the todo's card (TodoCard.tsx), dark as the notch
// is. It works through what waits on you with the same keys as a session there, from the same
// code (queue.ts, useWaitingQueue.ts), the todos among the sessions and reviews in the bar's order
// (bar.ts `notchRows`): ⇧⌘E marks it done and goes on, ⌃⌘1 to ⌃⌘7 snooze it and go on, ⌘⏎ in its
// box starts a session on it and goes on, ⌘] skips. The card follows the board while it's up, for
// the todo and what's around it: the pill carries only its id and title.

import { useCallback, useEffect, useRef, useState } from 'react';
import { followBoard, type Following } from '../followBoard';
import type { Left } from '../menus';
import type { Board } from '../types';
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
  onLeft: (key: string, done: Left) => void;
  /** The board no longer has it. */
  onGone: (key: string) => void;
}

export function NotchTodo({ todoKey, now, queue, notice, onBack, keys, onKeys, onClose, onLeft, onGone }: NotchTodoProps) {
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
  const todo = board?.todos.find((t) => t.id === id) ?? null;
  const gone = useRef(onGone);
  gone.current = onGone;
  useEffect(() => {
    if (board && !todo) gone.current(todoKey);
  }, [board, todo, todoKey]);

  const apps = board?.apps;
  const host = useCardHost({ sessionKey: null, anchor: 'due', live: { board, needBoard, there: undefined }, apps, terminal: apps?.terminal ?? 'Terminal', away: onClose });
  if (!board || !todo) return null;

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
        todos={board.todos}
        labels={board.labels ?? []}
        lanes={board.lanes}
        usage={board.usage}
        projects={board.projects.map((p) => p.name)}
        now={now}
        can={{
          report: host.report,
          left: (done) => onLeft(todoKey, done),
          showTodo: host.showTodo,
          showSession: host.showSession,
          showLane: host.showLane,
          openSession: (lane, session) => host.open(lane, session.openTarget),
        }}
        queue={queue}
        notice={notice}
        hotkeys={{ onBack, onClose }}
      />
    </div>
  );
}
