// The Todo view: work you mean to hand an agent. In progress, Ready and Waiting come in your
// order (drag, or ⌃⌘↑ ⌃⌘↓, to reorder), then Done under a fold. Waiting holds the todos marked as
// waiting and the ones not started that wait on a todo that isn't done. A row shows the todo's
// labels, what it waits on, the sessions on it with their status, and the worktree it's in.
// Type in the box at the top and press Return to add one. ⌘-click and ⇧-click pick several, to
// start in one session.

import { forwardRef, type ReactNode } from 'react';
import { absolute, ago, middleTruncate } from '../format';
import { attachmentsIn, blockedBy, SECTION_WORD, snippet, todoPr, type TodoItem, type TodoSection, type TodoSectionId } from '../todos';
import type { Label, Lane, Session, Todo, TodoHint, TodoStatus } from '../types';
import { HarnessChip, Icon, Kbd, PrChip } from './primitives';
import { LabelChip, TodoGlyph } from './TodoBits';
import { useDragOrder, type DragProps } from './useDragOrder';

/** Where a new todo will be linked, when it's made from a worktree or a session. */
export interface ComposerContext {
  worktree?: string;
  session?: string;
  /** How the chip names it. */
  label: string;
}

interface ComposerProps {
  value: string;
  onChange: (value: string) => void;
  context: ComposerContext | null;
  onClearContext: () => void;
  onCreate: (title: string) => void;
  /** Where it goes, when that isn't the project showing. */
  where: string | null;
}

/** The box at the top of the list. Return adds the todo and keeps the box for the next one. */
export const TodoComposer = forwardRef<HTMLInputElement, ComposerProps>(function TodoComposer({ value, onChange, context, onClearContext, onCreate, where }, ref) {
  return (
    <form
      className="ca-todonew"
      onSubmit={(e) => {
        e.preventDefault();
        if (value.trim()) onCreate(value.trim());
      }}
    >
      <Icon name="plus" size={14} />
      <input
        ref={ref}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={context ? 'What needs doing there?' : 'New todo'}
        aria-label="New todo"
        spellCheck
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            e.stopPropagation();
            onChange('');
            onClearContext();
            e.currentTarget.blur();
          }
        }}
      />
      {context && (
        <span className="ca-chip ca-todonew-ctx" title={`The new todo is linked to ${context.label}`}>
          <Icon name="link" size={12} />
          <span>{middleTruncate(context.label, 32)}</span>
          <button type="button" aria-label="Don't link it" onClick={onClearContext}>
            <Icon name="x" size={10} stroke={2.25} />
          </button>
        </span>
      )}
      {where && <span className="ca-todonew-where">in {where}</span>}
      {value.trim() ? <Kbd keys={['⏎']} /> : !context && <Kbd keys={['N']} />}
    </form>
  );
});

/** How a click selects a row: alone, ⌘ to add it or leave it out, ⇧ for the rows up to it. */
export type SelectHow = 'one' | 'toggle' | 'range';

interface TodoRowProps {
  item: TodoItem;
  /** The section it's listed in. */
  section: TodoSectionId;
  domId: string;
  now: number;
  selected: boolean;
  /** Show the project: the view spans all of them. */
  showProject: boolean;
  onSelect: (how: SelectHow) => void;
  onOpen: () => void;
  onContextMenu: () => void;
  onOpenSession: (lane: Lane, session: Session) => void;
  onFilterLabel: (label: Label) => void;
  /** The open todos it waits on. */
  blocked: Todo[];
  /** Jev says its work looks done. */
  hint?: TodoHint;
  drag?: DragProps;
}

const CHIPS_SHOWN = 3;

function TodoRow({ item, section, domId, now, selected, showProject, onSelect, onOpen, onContextMenu, onOpenSession, onFilterLabel, blocked, hint, drag }: TodoRowProps) {
  const { todo } = item;
  const pr = todoPr(item);
  const detail = snippet(todo.description);
  const sessions = item.sessions.slice(0, CHIPS_SHOWN);
  const more = item.sessions.length - sessions.length;
  const lanes = item.places.map((p) => p.lane);
  const waiting = todo.pending.length > 0;
  const attached = attachmentsIn(todo.description);
  return (
    <div
      id={domId}
      role="option"
      aria-selected={selected}
      className={['ca-lane', 'ca-todorow', `is-${todo.status}`, selected && 'is-selected', drag && 'is-draggable', drag?.extraClass].filter(Boolean).join(' ')}
      data-pin-index={drag?.pinIndex}
      onPointerDown={drag?.onPointerDown}
      onMouseDown={(e) => {
        // A right-click (or ⌃-click) leaves the selection to the menu: on a picked row, the menu
        // is for all of them.
        if (e.button !== 0 || e.ctrlKey) return;
        onSelect(e.metaKey ? 'toggle' : e.shiftKey ? 'range' : 'one');
      }}
      onDoubleClick={(e) => {
        if (!e.metaKey && !e.shiftKey) onOpen();
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu();
      }}
    >
      {drag && (
        <span className="ca-grip" aria-hidden="true">
          <Icon name="grip-vertical" size={10} />
        </span>
      )}
      <div className="ca-seg ca-seg-glyph">
        <TodoGlyph status={todo.status} waiting={section === 'waiting'} />
      </div>
      <div className="ca-seg ca-seg-main">
        <div className="ca-lane-line1">
          <span className="ca-todorow-title">{todo.title || 'Untitled'}</span>
          {detail && <span className="ca-lane-detail">{detail}</span>}
        </div>
        <div className="ca-lane-line2">
          <span className="ca-lane-agents">
            {showProject && todo.project && <span className="ca-tag">{todo.project}</span>}
            {item.labels.map((l) => (
              <LabelChip key={l.id} label={l} title={`Show only “${l.name}”`} onClick={() => onFilterLabel(l)} />
            ))}
            {todo.waiting && todo.status !== 'done' && (
              <span className="ca-chip ca-todorow-blocked" title={`Marked as waiting${todo.waiting.since > 0 ? ` ${ago(todo.waiting.since, now)} ago` : ''}${todo.waiting.note ? `: ${todo.waiting.note}` : '.'}`}>
                <Icon name="hourglass" size={12} />
                <span className="ca-todorow-blocked-text">{todo.waiting.note ? `Waiting for ${todo.waiting.note}` : 'Marked as waiting'}</span>
              </span>
            )}
            {blocked.length > 0 && todo.status !== 'done' && (
              <span className="ca-chip ca-todorow-blocked" title={`Waits on ${blocked.map((t) => `“${t.title || 'Untitled'}”`).join(', ')}, not done yet.`}>
                <Icon name="lock" size={12} />
                <span className="ca-todorow-blocked-text">{blocked.length === 1 ? `Waiting on ${blocked[0].title || 'Untitled'}` : `Waiting on ${blocked.length} todos`}</span>
              </span>
            )}
            {hint && todo.status !== 'done' && (
              <span className="ca-chip ca-todorow-done" title={`Looks done: ${hint.why}. Mark it done, or say not yet, in the panel.`}>
                <Icon name="check" size={12} />
                Looks done
              </span>
            )}
            {attached.length > 0 && (
              <span className="ca-chip ca-todorow-attached" title={attached.map((a) => (a.kind === 'image' ? 'Image' : a.name)).join('\n')}>
                <Icon name="paperclip" size={12} />
                {attached.length}
              </span>
            )}
            {sessions.map(({ lane, session }) => (
              <HarnessChip key={session.key} session={session} onOpen={() => onOpenSession(lane, session)} />
            ))}
            {more > 0 && <span className="ca-chip ca-chip--more">+{more}</span>}
            {waiting && (
              <span className="ca-chip ca-todorow-waiting" title="Started in an app. The session is linked when it shows up.">
                <Icon name="clock" size={12} />
                Waiting for the session
              </span>
            )}
          </span>
          <span className="ca-lane-right">
            {lanes.slice(0, 2).map((l) => (
              <span key={l.id} className="ca-todorow-lane" title={l.path}>
                <Icon name="branch" size={12} />
                {middleTruncate(l.callsign, 24)}
              </span>
            ))}
            {lanes.length > 2 && <span className="ca-chip ca-chip--more">+{lanes.length - 2}</span>}
            {pr && <PrChip pr={pr} compact />}
          </span>
        </div>
      </div>
      <div className="ca-seg ca-seg-age" title={absolute(item.lastActivity)}>
        {ago(item.lastActivity, now)}
      </div>
    </div>
  );
}

interface TodoViewProps {
  sections: TodoSection[];
  /** Every todo, in any project: what a row waits on can be anywhere. */
  todos: Todo[];
  now: number;
  selectedKey: string | null;
  /** The rows picked together with the selected one, when there are several. */
  picked: Set<string>;
  domId: (row: TodoItem) => string;
  showProject: boolean;
  /** Dragging reorders a section; not while a search or a filter hides some of it. */
  canReorder: boolean;
  onSelect: (row: TodoItem, how: SelectHow) => void;
  onOpen: (row: TodoItem) => void;
  onContextMenu: (row: TodoItem) => void;
  onOpenSession: (lane: Lane, session: Session) => void;
  onFilterLabel: (label: Label) => void;
  /** Moves a todo to insertion point `index` among the section's rows. */
  onMove: (section: TodoSectionId, id: string, index: number) => void;
  showDone: boolean;
  onToggleDone: () => void;
  composer: ReactNode;
  /** What to say when there's nothing to show. */
  empty: ReactNode;
  /** Todos Jev says look done, by id. */
  hints?: Record<string, TodoHint>;
}

/** Each section's box: its todos' status, dashed for Waiting. */
const SECTION_STATUS: Record<TodoSectionId, TodoStatus> = { 'in-progress': 'in-progress', ready: 'todo', waiting: 'todo', done: 'done' };

function SectionHead({ id, count, hint }: { id: TodoSectionId; count: number; hint?: ReactNode }) {
  return (
    <div className="ca-bay ca-bay--section" role="heading" aria-level={3}>
      <TodoGlyph status={SECTION_STATUS[id]} waiting={id === 'waiting'} size={11} title="" />
      <span>{SECTION_WORD[id]}</span>
      <span className="ca-bay-count">{count}</span>
      {hint && <span className="ca-bay-action ca-bay-hint">{hint}</span>}
    </div>
  );
}

function Section({ section, props }: { section: TodoSection; props: TodoViewProps }) {
  const { todos, now, selectedKey, picked, domId, showProject, canReorder, onSelect, onOpen, onContextMenu, onOpenSession, onFilterLabel, onMove } = props;
  const { listRef, dragging, rowProps } = useDragOrder((id, index) => onMove(section.id, id, index));
  const draggable = canReorder && section.rows.length > 1;
  return (
    <section className="ca-bayblock">
      <SectionHead
        id={section.id}
        count={section.rows.length}
        hint={
          draggable && selectedKey && picked.size === 0 && section.rows.some((r) => r.key === selectedKey) ? (
            <>
              Drag or <Kbd keys={['⌃', '⌘', '↑']} /> <Kbd keys={['⌃', '⌘', '↓']} /> to reorder
            </>
          ) : undefined
        }
      />
      <div ref={listRef} className={['ca-bay-lanes', dragging && 'is-dragging'].filter(Boolean).join(' ')}>
        {section.rows.map((r, i) => (
          <TodoRow
            key={r.key}
            item={r}
            section={section.id}
            domId={domId(r)}
            now={now}
            selected={r.key === selectedKey || picked.has(r.key)}
            showProject={showProject}
            onSelect={(how) => onSelect(r, how)}
            onOpen={() => onOpen(r)}
            onContextMenu={() => onContextMenu(r)}
            onOpenSession={onOpenSession}
            onFilterLabel={onFilterLabel}
            blocked={blockedBy(r.todo, todos)}
            hint={props.hints?.[r.todo.id]}
            drag={draggable ? rowProps(r.todo.id, i, section.rows.length) : undefined}
          />
        ))}
      </div>
    </section>
  );
}

export function TodoView(props: TodoViewProps) {
  const { sections, todos, now, selectedKey, picked, domId, showProject, onSelect, onOpen, onContextMenu, onOpenSession, onFilterLabel, showDone, onToggleDone, composer, empty } = props;
  const open = sections.filter((s) => s.id !== 'done');
  const done = sections.find((s) => s.id === 'done');
  return (
    <>
      {composer}
      {sections.length === 0 && <div className="ca-board-empty">{empty}</div>}
      {open.map((s) => (
        <Section key={s.id} section={s} props={props} />
      ))}
      {done && (
        <section className="ca-bayblock ca-todo-done">
          <SectionHead
            id="done"
            count={done.rows.length}
            hint={
              <button type="button" className="ca-linkbtn" aria-expanded={showDone} onClick={onToggleDone}>
                {showDone ? 'Hide' : 'Show'}
              </button>
            }
          />
          {showDone && (
            <div className="ca-bay-lanes">
              {done.rows.map((r) => (
                <TodoRow
                  key={r.key}
                  item={r}
                  section="done"
                  domId={domId(r)}
                  now={now}
                  selected={r.key === selectedKey || picked.has(r.key)}
                  showProject={showProject}
                  onSelect={(how) => onSelect(r, how)}
                  onOpen={() => onOpen(r)}
                  onContextMenu={() => onContextMenu(r)}
                  onOpenSession={onOpenSession}
                  onFilterLabel={onFilterLabel}
                  blocked={blockedBy(r.todo, todos)}
                />
              ))}
            </div>
          )}
        </section>
      )}
    </>
  );
}
