// Small pieces for todos: the status box, the chip a session or worktree shows for its todo,
// label chips and the due date's chip. A todo's box is square, so it never reads as a session's
// round status glyph; a waiting todo's box is dashed.

import { dueWords } from '../due';
import type { Label, Todo, TodoStatus } from '../types';
import { STATUS_WORD } from '../todos';
import { Icon, inRowOpen } from './primitives';

export function TodoGlyph({ status, waiting = false, size = 12, title }: { status: TodoStatus; waiting?: boolean; size?: number; title?: string }) {
  const word = waiting ? (status === 'in-progress' ? 'In progress, waiting' : 'Waiting') : STATUS_WORD[status];
  return (
    <svg className={['ca-todoglyph', `is-${status}`, waiting && 'is-waiting'].filter(Boolean).join(' ')} width={size} height={size} viewBox="0 0 12 12" role="img" aria-label={title ?? word}>
      <rect className="ca-tg-box" x="1.5" y="1.5" width="9" height="9" rx="2.25" />
      {status === 'in-progress' &&
        // Inside a dashed box the fill keeps clear of the dashes, so they still read as dashes.
        (waiting ? <rect className="ca-tg-fill" x="3.6" y="6" width="4.8" height="2.4" rx="0.6" /> : <path className="ca-tg-fill" d="M1.5 6h9v2.25a2.25 2.25 0 0 1 -2.25 2.25h-4.5a2.25 2.25 0 0 1 -2.25 -2.25z" />)}
      {status === 'done' && <path className="ca-tg-check" d="M3.75 6.1l1.55 1.55l2.95 -3.1" />}
    </svg>
  );
}

export function LabelDot({ label }: { label: Label }) {
  return <i className={`ca-labeldot is-${label.color}`} aria-hidden="true" />;
}

/** A label. With `onClick`, a click runs it; with `inRow` too, it's on a list's row, where a click
 *  selects the row, so a double-click does. */
export function LabelChip({ label, onClick, title, inRow = false }: { label: Label; onClick?: () => void; title?: string; inRow?: boolean }) {
  const inner = (
    <>
      <LabelDot label={label} />
      <span>{label.name}</span>
    </>
  );
  if (!onClick || inRow) {
    return (
      <span className="ca-labelchip" title={title} onDoubleClick={onClick && inRowOpen(onClick)}>
        {inner}
      </span>
    );
  }
  return (
    <button
      type="button"
      className="ca-labelchip ca-labelchip--button"
      title={title}
      onMouseDown={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onClick={onClick}
    >
      {inner}
    </button>
  );
}

/** A todo on a session's or a worktree's row. Click it to show the todo, or double-click it with
 *  `inRow`, on a list's row, where a click selects the row. */
export function TodoChip({ todo, onShow, inRow = false }: { todo: Todo; onShow?: (todo: Todo) => void; inRow?: boolean }) {
  const title = todo.title || 'Untitled todo';
  const tip = `Todo · ${STATUS_WORD[todo.status]}: ${title}`;
  if (!onShow || inRow) {
    return (
      <span className="ca-chip ca-todochip" title={onShow ? `${tip}. Double-click to show it in Todo.` : tip} onDoubleClick={onShow && inRowOpen(() => onShow(todo))}>
        <TodoGlyph status={todo.status} size={10} />
        <span className="ca-todochip-title">{title}</span>
      </span>
    );
  }
  return (
    <button
      type="button"
      className="ca-chip ca-chip--button ca-todochip"
      title={`${tip}. Click to show it in Todo.`}
      onMouseDown={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onClick={() => onShow(todo)}
    >
      <TodoGlyph status={todo.status} size={10} />
      <span className="ca-todochip-title">{title}</span>
    </button>
  );
}

/** A row's todo chips: the first one, and how many more. Open todos come before done ones. */
export function TodoChips({ todos, onShow, inRow = false }: { todos: Todo[] | undefined; onShow?: (todo: Todo) => void; inRow?: boolean }) {
  if (!todos || todos.length === 0) return null;
  const sorted = [...todos].sort((a, b) => Number(a.status === 'done') - Number(b.status === 'done'));
  return (
    <>
      <TodoChip todo={sorted[0]} onShow={onShow} inRow={inRow} />
      {sorted.length > 1 && (
        <span className="ca-chip ca-chip--more" title={sorted.slice(1).map((t) => t.title || 'Untitled todo').join('\n')}>
          +{sorted.length - 1}
        </span>
      )}
    </>
  );
}

/** When an open todo is due: red once the day has passed, amber on the day. A done todo's date
 *  isn't shown: it no longer asks anything of you. */
export function DueChip({ todo, now, className }: { todo: Todo; now: number; className?: string }) {
  const said = todo.due && todo.status !== 'done' ? dueWords(todo.due, now) : null;
  if (!said) return null;
  return (
    <span className={['ca-chip', 'ca-duechip', `is-${said.tone}`, className].filter(Boolean).join(' ')} title={`Due ${said.long}`}>
      <Icon name="calendar" size={12} />
      {said.text}
    </span>
  );
}
