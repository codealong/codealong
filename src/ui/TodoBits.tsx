// Small pieces for todos: the status box, the chip a session or worktree shows for its todo,
// and label chips. A todo's box is square, so it never reads as a session's round status glyph.

import type { Label, Todo, TodoStatus } from '../types';
import { STATUS_WORD } from '../todos';

export function TodoGlyph({ status, size = 12, title }: { status: TodoStatus; size?: number; title?: string }) {
  return (
    <svg className={`ca-todoglyph is-${status}`} width={size} height={size} viewBox="0 0 12 12" role="img" aria-label={title ?? STATUS_WORD[status]}>
      <rect className="ca-tg-box" x="1.5" y="1.5" width="9" height="9" rx="2.25" />
      {status === 'in-progress' && <path className="ca-tg-fill" d="M1.5 6h9v2.25a2.25 2.25 0 0 1 -2.25 2.25h-4.5a2.25 2.25 0 0 1 -2.25 -2.25z" />}
      {status === 'done' && <path className="ca-tg-check" d="M3.75 6.1l1.55 1.55l2.95 -3.1" />}
    </svg>
  );
}

export function LabelDot({ label }: { label: Label }) {
  return <i className={`ca-labeldot is-${label.color}`} aria-hidden="true" />;
}

export function LabelChip({ label, onClick, title }: { label: Label; onClick?: () => void; title?: string }) {
  const inner = (
    <>
      <LabelDot label={label} />
      <span>{label.name}</span>
    </>
  );
  if (!onClick) {
    return (
      <span className="ca-labelchip" title={title}>
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

/** A todo on a session's or a worktree's row. Click it to show the todo. */
export function TodoChip({ todo, onShow }: { todo: Todo; onShow?: (todo: Todo) => void }) {
  const title = todo.title || 'Untitled todo';
  const tip = `Todo · ${STATUS_WORD[todo.status]}: ${title}`;
  if (!onShow) {
    return (
      <span className="ca-chip ca-todochip" title={tip}>
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
export function TodoChips({ todos, onShow }: { todos: Todo[] | undefined; onShow?: (todo: Todo) => void }) {
  if (!todos || todos.length === 0) return null;
  const sorted = [...todos].sort((a, b) => Number(a.status === 'done') - Number(b.status === 'done'));
  return (
    <>
      <TodoChip todo={sorted[0]} onShow={onShow} />
      {sorted.length > 1 && (
        <span className="ca-chip ca-chip--more" title={sorted.slice(1).map((t) => t.title || 'Untitled todo').join('\n')}>
          +{sorted.length - 1}
        </span>
      )}
    </>
  );
}
