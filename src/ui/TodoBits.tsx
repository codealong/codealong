// Small pieces for todos: the status box, the chip a session or worktree shows for its todo,
// label chips and the due date's chip. A todo's box is square, so it never reads as a session's
// round status glyph; a waiting todo's box is dashed, and one in review is filled, its work done
// and not yet checked off.

import type { MouseEvent } from 'react';
import { dueWords } from '../due';
import type { Label, Todo, TodoStatus } from '../types';
import { STATUS_WORD } from '../todos';
import { Chip, cx, Icon, MoreChip } from './primitives';

export function TodoGlyph({ status, waiting = false, review = false, size = 12, title }: { status: TodoStatus; waiting?: boolean; review?: boolean; size?: number; title?: string }) {
  const word = waiting ? (status === 'in-progress' ? 'In progress, waiting' : 'Waiting') : review ? 'In review' : STATUS_WORD[status];
  return (
    <svg className={cx('ca-todoglyph', `is-${status}`, waiting && 'is-waiting', review && 'is-review')} width={size} height={size} viewBox="0 0 12 12" role="img" aria-label={title ?? word}>
      <rect className="ca-tg-box" x="1.5" y="1.5" width="9" height="9" rx="2.25" />
      {status === 'in-progress' &&
        !review &&
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
 *  selects the row, so a double-click does. `remove` gives it a ✕ that takes it off; `suggested`
 *  is one Jev offers, which reads as a question until it's clicked. */
export function LabelChip({ label, onClick, title, inRow = false, remove, suggested = false }: { label: Label; onClick?: () => void; title?: string; inRow?: boolean; remove?: () => void; suggested?: boolean }) {
  return (
    <Chip kind="label" className={suggested ? 'ca-suggestchip' : undefined} title={title} act={onClick} inRow={inRow} remove={remove && { label: `Remove ${label.name}`, title: `Remove ${label.name}`, run: remove }}>
      {suggested && <Icon name="sparkles" size={11} />}
      <LabelDot label={label} />
      <span>
        {label.name}
        {suggested && '?'}
      </span>
    </Chip>
  );
}

/** The dashed chip that adds one (a label, a due date, what a todo waits on, a link): it opens the
 *  picker or menu under it, and closes a picker again. */
export function AddChip({ icon, label, title, open, onToggle }: { icon: string; label: string; title?: string; open: boolean; onToggle: (e: MouseEvent<HTMLButtonElement>) => void }) {
  return (
    <button type="button" className="ca-labelchip ca-labelchip--add" aria-expanded={open} title={title} onClick={onToggle}>
      <Icon name={icon} size={12} />
      <span>{label}</span>
    </button>
  );
}

/** A todo on a session's or a worktree's row. Click it to show the todo, or double-click it with
 *  `inRow`, on a list's row, where a click selects the row. */
export function TodoChip({ todo, onShow, inRow = false }: { todo: Todo; onShow?: (todo: Todo) => void; inRow?: boolean }) {
  const title = todo.title || 'Untitled todo';
  return (
    <Chip className="ca-todochip" title={`Todo · ${STATUS_WORD[todo.status]}: ${title}`} does="show it in Todo" act={onShow && (() => onShow(todo))} inRow={inRow}>
      <TodoGlyph status={todo.status} size={10} />
      <span className="ca-todochip-title">{title}</span>
    </Chip>
  );
}

/** A row's todo chips: the first one, and how many more. Open todos come before done ones. */
export function TodoChips({ todos, onShow, inRow = false }: { todos: Todo[] | undefined; onShow?: (todo: Todo) => void; inRow?: boolean }) {
  if (!todos || todos.length === 0) return null;
  const sorted = [...todos].sort((a, b) => Number(a.status === 'done') - Number(b.status === 'done'));
  return (
    <>
      <TodoChip todo={sorted[0]} onShow={onShow} inRow={inRow} />
      <MoreChip names={sorted.slice(1).map((t) => t.title || 'Untitled todo')} />
    </>
  );
}

/** When an open todo is due: red once the day has passed, amber on the day. A done todo's date
 *  isn't shown: it no longer asks anything of you. */
export function DueChip({ todo, now, className }: { todo: Todo; now: number; className?: string }) {
  const said = todo.due && todo.status !== 'done' ? dueWords(todo.due, now) : null;
  if (!said) return null;
  return (
    <Chip className={cx('ca-duechip', `is-${said.tone}`, className)} title={`Due ${said.long}`}>
      <Icon name="calendar" size={12} />
      {said.text}
    </Chip>
  );
}
