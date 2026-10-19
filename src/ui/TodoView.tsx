// The Todo view: work you mean to hand an agent. In progress, In review, Ready and Waiting come
// in your order (drag, or ⌃⌘↑ ⌃⌘↓, to reorder), then Done under a fold. In review holds the
// todos marked as ready for review and the ones whose PR is with its reviewers. A todo dragged out of the window
// is pinned to the desktop (useRowDrag.tsx). Waiting holds the todos marked as
// waiting and the ones not started that wait on a todo that isn't done. A row shows when the todo
// is due, its labels, what it waits on, the sessions on it with their status, and the worktree
// it's in.
// Type in the box at the top and press Return to add one. ⌘-click and ⇧-click pick several, to
// start in one session. Rows, in the bar over the list, puts each todo on one line (`TODO_ROWS`).

import { forwardRef, type ReactNode } from 'react';
import { absolute, ago, middleTruncate } from '../format';
import { selectHow, type SelectHow } from '../picks';
import { attachmentsIn, blockedBy, SECTION_WORD, snippet, todoPr, type TodoItem, type TodoSection, type TodoSectionId } from '../todos';
import type { Label, Lane, Pr, Session, Todo, TodoHint, TodoStatus } from '../types';
import { Chip, cx, HarnessChip, Icon, Kbd, MoreChip, PrChip, ReorderHint, Row, WorktreeChip } from './primitives';
import { rowsOptions, type RowLines } from './ListBar';
import { DueChip, LabelChip, TodoGlyph } from './TodoBits';
import { useRowDrag, type DragProps } from './useRowDrag';
import { cmd } from '../platform';

/** How much each todo's row says: when it's due, its labels, sessions, worktree and PR on a second
 *  line, or one line with them at the end, as many as fit. */
export const TODO_ROWS = rowsOptions('under each todo, when it’s due, its labels, sessions, worktree and PR', 'each todo with its chips at the end, as many as fit, and its PR');

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
        <Chip className="ca-todonew-ctx" title={`The new todo is linked to ${context.label}`} remove={{ label: "Don't link it", run: onClearContext }}>
          <Icon name="link" size={12} />
          <span>{middleTruncate(context.label, 32)}</span>
        </Chip>
      )}
      {where && <span className="ca-todonew-where">in {where}</span>}
      {value.trim() ? <Kbd keys={['⏎']} /> : !context && <Kbd keys={['N']} />}
    </form>
  );
});

interface TodoRowProps {
  item: TodoItem;
  /** The section it's listed in. */
  section: TodoSectionId;
  domId: string;
  now: number;
  /** On one line: its title and description, then its chips at the end. */
  slim: boolean;
  selected: boolean;
  /** Show the project: the view spans all of them. */
  showProject: boolean;
  onSelect: (how: SelectHow) => void;
  onOpen: () => void;
  onContextMenu: () => void;
  onOpenSession: (lane: Lane, session: Session) => void;
  /** A click on its PR's chip: the PR in the panel beside the list. A double-click opens it on
   *  GitHub (`onOpenPr`). */
  onShowPr: (pr: Pr) => void;
  onOpenPr: (pr: Pr) => void;
  onFilterLabel: (label: Label) => void;
  /** The open todos it waits on. */
  blocked: Todo[];
  /** Jev says its work looks done. */
  hint?: TodoHint;
  drag?: DragProps;
}

const CHIPS_SHOWN = 3;

/** A todo in the list: its title and the start of its description, then when it's due, its labels,
 *  what it waits on, the sessions on it and the worktrees and PR it's in. On one line (`slim`)
 *  they follow its title at the line's end, the last giving way first, its PR always shown. */
function TodoRow({ item, section, domId, now, slim, selected, showProject, onSelect, onOpen, onContextMenu, onOpenSession, onShowPr, onOpenPr, onFilterLabel, blocked, hint, drag }: TodoRowProps) {
  const { todo } = item;
  const pr = todoPr(item);
  const detail = snippet(todo.description);
  const sessions = item.sessions.slice(0, CHIPS_SHOWN);
  const lanes = item.places.map((p) => p.lane);
  const waiting = todo.pending.length > 0;
  const attached = attachmentsIn(todo.description);
  const chips = (
    <>
      {showProject && todo.project && <span className="ca-tag">{todo.project}</span>}
      <DueChip todo={todo} now={now} />
      {item.labels.map((l) => (
        <LabelChip key={l.id} label={l} title={`Double-click to show only “${l.name}”`} onClick={() => onFilterLabel(l)} inRow />
      ))}
      {todo.waiting && todo.status !== 'done' && (
        <Chip className="ca-todorow-blocked" title={`Marked as waiting${todo.waiting.since > 0 ? ` ${ago(todo.waiting.since, now)} ago` : ''}${todo.waiting.note ? `: ${todo.waiting.note}` : '.'}`}>
          <Icon name="hourglass" size={12} />
          <span className="ca-todorow-blocked-text">{todo.waiting.note ? `Waiting for ${todo.waiting.note}` : 'Marked as waiting'}</span>
        </Chip>
      )}
      {blocked.length > 0 && todo.status !== 'done' && (
        <Chip className="ca-todorow-blocked" title={`Waits on ${blocked.map((t) => `“${t.title || 'Untitled'}”`).join(', ')}, not done yet.`}>
          <Icon name="lock" size={12} />
          <span className="ca-todorow-blocked-text">{blocked.length === 1 ? `Waiting on ${blocked[0].title || 'Untitled'}` : `Waiting on ${blocked.length} todos`}</span>
        </Chip>
      )}
      {todo.review && todo.status !== 'done' && (
        <Chip className="ca-todorow-review" title={`Marked as ready for review${todo.review.since > 0 ? ` ${ago(todo.review.since, now)} ago` : ''}${todo.review.note ? `: ${todo.review.note}` : '.'}`}>
          <Icon name="eye" size={12} />
          <span className="ca-todorow-blocked-text">{todo.review.note ? `Review ${todo.review.note}` : 'Ready for review'}</span>
        </Chip>
      )}
      {hint && todo.status !== 'done' && (
        <Chip className="ca-todorow-done" title={`Looks done: ${hint.why}. Mark it done, or say not yet, in the panel.`}>
          <Icon name="check" size={12} />
          Looks done
        </Chip>
      )}
      {attached.length > 0 && (
        <Chip className="ca-todorow-attached" title={attached.map((a) => (a.kind === 'image' ? 'Image' : a.name)).join('\n')}>
          <Icon name="paperclip" size={12} />
          {attached.length}
        </Chip>
      )}
      {sessions.map(({ lane, session }) => (
        <HarnessChip key={session.key} session={session} named onOpen={() => onOpenSession(lane, session)} />
      ))}
      <MoreChip names={item.sessions.slice(CHIPS_SHOWN).map(({ session }) => `${session.title} · ${session.account}`)} />
      {waiting && (
        <Chip className="ca-todorow-waiting" title="Started in an app. The session is linked when it shows up.">
          <Icon name="clock" size={12} />
          Waiting for the session
        </Chip>
      )}
    </>
  );
  const places = (
    <>
      {lanes.slice(0, 2).map((l) => (
        <WorktreeChip key={l.id} lane={l} short />
      ))}
      <MoreChip names={lanes.slice(2).map((l) => l.callsign)} />
    </>
  );
  const prChip = pr && <PrChip pr={pr} compact onShow={() => onShowPr(pr)} onOpen={() => onOpenPr(pr)} inRow />;
  return (
    <Row
      id={domId}
      selected={selected}
      className={cx('ca-lane', 'is-item', 'ca-todorow', slim && 'is-slim', `is-${todo.status}`)}
      drag={drag}
      // ⌘ adds it to the ones picked or leaves it out, ⇧ picks the rows up to it. A right-click
      // leaves the selection alone: on a picked row, the menu is for all of them.
      onSelect={(e) => onSelect(selectHow(e))}
      onOpen={(e) => {
        if (!cmd(e) && !e.shiftKey) onOpen();
      }}
      onMenu={onContextMenu}
    >
      <div className="ca-seg ca-seg-glyph">
        <TodoGlyph status={todo.status} waiting={section === 'waiting'} review={section === 'review'} />
      </div>
      <div className="ca-seg ca-seg-main">
        <div className="ca-lane-line1">
          <span className="ca-lane-headline ca-todorow-title">{todo.title || 'Untitled'}</span>
          {detail && <span className="ca-lane-detail">{detail}</span>}
          {slim && (
            <>
              <span className="ca-chips ca-lane-tags">
                {chips}
                {places}
              </span>
              {prChip}
            </>
          )}
        </div>
        {!slim && (
          <div className="ca-lane-line2">
            <span className="ca-chips ca-lane-lead">{chips}</span>
            <span className="ca-lane-right">
              {places}
              {prChip}
            </span>
          </div>
        )}
      </div>
      <div className="ca-seg ca-seg-age" title={absolute(item.lastActivity)}>
        {ago(item.lastActivity, now)}
      </div>
    </Row>
  );
}

interface TodoViewProps {
  sections: TodoSection[];
  /** Every todo, in any project: what a row waits on can be anywhere. */
  todos: Todo[];
  /** Two lines a todo, or one. */
  lines: RowLines;
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
  /** A click on a row's PR chip: the PR in the panel beside the list, the row selected. A
   *  double-click opens it on GitHub (`onOpenPr`). */
  onShowPr: (pr: Pr, row: TodoItem) => void;
  onOpenPr: (pr: Pr) => void;
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

/** Each section's box: its todos' status, dashed for Waiting, filled for In review. */
const SECTION_STATUS: Record<TodoSectionId, TodoStatus> = { 'in-progress': 'in-progress', review: 'in-progress', ready: 'todo', waiting: 'todo', done: 'done' };

function SectionHead({ id, count, hint }: { id: TodoSectionId; count: number; hint?: ReactNode }) {
  return (
    <div className="ca-bay ca-bay--section" role="heading" aria-level={3}>
      <TodoGlyph status={SECTION_STATUS[id]} waiting={id === 'waiting'} review={id === 'review'} size={11} title="" />
      <span>{SECTION_WORD[id]}</span>
      <span className="ca-bay-count">{count}</span>
      {hint && <span className="ca-bay-action ca-bay-hint">{hint}</span>}
    </div>
  );
}

function Section({ section, props }: { section: TodoSection; props: TodoViewProps }) {
  const { todos, lines, now, selectedKey, picked, domId, showProject, canReorder, onSelect, onOpen, onContextMenu, onOpenSession, onShowPr, onOpenPr, onFilterLabel, onMove } = props;
  const drag = useTodoDrag(section.rows, (id, index) => onMove(section.id, id, index));
  const draggable = canReorder && section.rows.length > 1;
  return (
    <section className="ca-bayblock">
      <SectionHead
        id={section.id}
        count={section.rows.length}
        hint={draggable && selectedKey && picked.size === 0 && section.rows.some((r) => r.key === selectedKey) ? <ReorderHint /> : undefined}
      />
      <div className="ca-bay-lanes" data-drag-list>
        {section.rows.map((r, i) => (
          <TodoRow
            key={r.key}
            item={r}
            section={section.id}
            domId={domId(r)}
            now={now}
            slim={lines === 'one'}
            selected={r.key === selectedKey || picked.has(r.key)}
            showProject={showProject}
            onSelect={(how) => onSelect(r, how)}
            onOpen={() => onOpen(r)}
            onContextMenu={() => onContextMenu(r)}
            onOpenSession={onOpenSession}
            onShowPr={(pr) => onShowPr(pr, r)}
            onOpenPr={onOpenPr}
            onFilterLabel={onFilterLabel}
            blocked={blockedBy(r.todo, todos)}
            hint={props.hints?.[r.todo.id]}
            drag={drag.rowProps(r.todo.id, draggable ? i : null, section.rows.length)}
          />
        ))}
      </div>
      {drag.ghost}
    </section>
  );
}

/** Drags for a section's todos: into a new order when `onMove` is given, and out of the window
 *  onto the desktop. */
function useTodoDrag(rows: TodoItem[], onMove?: (id: string, index: number) => void) {
  return useRowDrag({
    onPlace: onMove,
    desktop: (id) => ({ ref: 'todo', key: id }),
    label: (id) => rows.find((r) => r.todo.id === id)?.todo.title || 'Untitled',
  });
}

export function TodoView(props: TodoViewProps) {
  const { sections, todos, lines, now, selectedKey, picked, domId, showProject, onSelect, onOpen, onContextMenu, onOpenSession, onShowPr, onOpenPr, onFilterLabel, showDone, onToggleDone, composer, empty } = props;
  const open = sections.filter((s) => s.id !== 'done');
  const done = sections.find((s) => s.id === 'done');
  // Done todos keep no order of yours; they can still go to the desktop.
  const doneDrag = useTodoDrag(done?.rows ?? []);
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
                  slim={lines === 'one'}
                  selected={r.key === selectedKey || picked.has(r.key)}
                  showProject={showProject}
                  onSelect={(how) => onSelect(r, how)}
                  onOpen={() => onOpen(r)}
                  onContextMenu={() => onContextMenu(r)}
                  onOpenSession={onOpenSession}
                  onShowPr={(pr) => onShowPr(pr, r)}
                  onOpenPr={onOpenPr}
                  onFilterLabel={onFilterLabel}
                  blocked={blockedBy(r.todo, todos)}
                  drag={doneDrag.rowProps(r.todo.id, null, 0)}
                />
              ))}
            </div>
          )}
          {doneDrag.ghost}
        </section>
      )}
    </>
  );
}
