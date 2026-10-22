// A todo in the right panel: its title, status and labels, when it's due, what it waits on (other
// todos, and what it's marked as waiting for), the description with its screenshots, and the
// worktrees and sessions it's linked to, as chips or with their details as a tree, as a PR's panel
// shows them (PanelSession.tsx `PanelPlaces`).
// Everything saves as you go. Start session opens the new-session sheet with the todo as the
// prompt, and the session it starts is linked to the todo.
// With `[jev] todos` on, Jev's suggestions for its project, labels and what it waits on show as
// chips to click, and a todo whose linked work looks done says so, with Mark done and Not yet.
// With several todos picked in the list, `TodoPickPanel` shows them instead, to start in one
// session, in the pick panel every list's picks share (PickPanel.tsx).

import { useEffect, useRef, useState, type ReactNode } from 'react';
import { suggestTodo } from '../api';
import { below } from '../contextMenu';
import { dayKey, dueChoices, dueDate, dueWords } from '../due';
import { absolute, ago, middleTruncate } from '../format';
import { branchLabel } from '../lanes';
import { blockedBy, cantWaitOn, dependents, depsOf, isOpen, itemSection, newId, nextColor, SECTION_WORD, sectionOf, STATUSES, STATUS_WORD, todoSuggestions, waitsOnTodos, worthSuggesting, type TodoItem, type TodoOp } from '../todos';
import type { Label, Lane, Session, Todo, TodoHint, TodoSuggested } from '../types';
import { DescriptionEditor } from './DescriptionEditor';
import { DetailsButton, PanelPlaces, TODO_PRIMARY, useDetailsOpen } from './PanelSession';
import { PickList, PickPanel, PickRow } from './PickPanel';
import { Button, Chip, cx, headerMenu, Icon, Kbd, Seg } from './primitives';
import { AddChip, LabelChip, LabelDot, TodoGlyph } from './TodoBits';
import { useDismiss } from './useDismiss';
import { keysIn } from '../platform';

/** Whether a todo's worktrees and sessions show their details, for every todo, as you left them. */
const PLACES_KEY = 'codealong.todoPlacesOpen';

/** How long the title and description have to rest before Jev is asked about them. */
const SUGGEST_AFTER_MS = 800;

/** Jev's suggestions for a todo, asked once its title says something and typing has paused, and
 *  again when the title, description or project change. A failure (no key, off) shows nothing. */
function useSuggested(todo: Todo, on: boolean): TodoSuggested | null {
  const [got, setGot] = useState<{ for: string; s: TodoSuggested } | null>(null);
  const key = on && isOpen(todo) && worthSuggesting(todo) ? [todo.id, todo.project, todo.title, todo.description].join('\n') : null;
  const latest = useRef(todo);
  latest.current = todo;
  useEffect(() => {
    if (!key) return;
    let live = true;
    const id = window.setTimeout(() => {
      const t = latest.current;
      suggestTodo(t.id, t.title, t.description, t.project)
        .then((s) => live && setGot({ for: key, s }))
        // The engine answers with an error when there's nothing to ask (one project, no labels,
        // no other todos) as well as when Jev fails, so a failure can't be told from nothing to
        // say: the chips just don't come.
        .catch(() => {});
    }, SUGGEST_AFTER_MS);
    return () => {
      live = false;
      window.clearTimeout(id);
    };
  }, [key]);
  return got && got.for === key ? got.s : null;
}

/** The title, as a heading you can type in. Return or leaving it saves. */
function TitleField({ value, onSave }: { value: string; onSave: (title: string) => void }) {
  const [draft, setDraft] = useState(value);
  const [focused, setFocused] = useState(false);
  const ref = useRef<HTMLTextAreaElement>(null);
  // The latest save, so a new board (and with it a new callback) doesn't restart the wait.
  const save = useRef(onSave);
  save.current = onSave;
  // Escape puts the title back; the blur that follows mustn't save what it replaced.
  const cancelled = useRef(false);
  const latest = useRef({ draft, value, focused });
  latest.current = { draft, value, focused };
  useEffect(() => {
    if (!focused) setDraft(value);
  }, [value, focused]);
  useEffect(() => {
    if (!focused || draft.trim() === value.trim()) return;
    const id = window.setTimeout(() => save.current(draft), 600);
    return () => window.clearTimeout(id);
  }, [draft, focused, value]);
  // Selecting another todo swaps the panel before this field sees a blur: save what's typed.
  useEffect(
    () => () => {
      const l = latest.current;
      if (l.focused && !cancelled.current && l.draft.trim() !== l.value.trim()) save.current(l.draft);
    },
    [],
  );
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${el.scrollHeight}px`;
  }, [draft]);
  return (
    <textarea
      ref={ref}
      className="ca-todoinsp-title"
      value={draft}
      rows={1}
      spellCheck
      placeholder="Untitled"
      aria-label="Title"
      onFocus={() => {
        cancelled.current = false;
        setFocused(true);
      }}
      onChange={(e) => setDraft(e.target.value.replace(/\n/g, ' '))}
      onBlur={() => {
        setFocused(false);
        if (!cancelled.current && draft.trim() !== value.trim()) onSave(draft);
      }}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === 'Escape') {
          e.preventDefault();
          e.stopPropagation();
          if (e.key === 'Escape') {
            cancelled.current = true;
            setDraft(value);
          }
          ref.current?.blur();
        }
      }}
    />
  );
}

/** A picker under the chip that opens it: it goes on a press anywhere else and on Escape; a
 *  press on its own chip, beside it in the same anchor, is the chip's, which closes it itself. */
function PickerBox({ label, className, onClose, children }: { label: string; className?: string; onClose: () => void; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useDismiss(ref, onClose, { inside: (t) => !!ref.current?.parentElement?.contains(t) });
  return (
    <div className={cx('ca-menu ca-labelpick', className)} ref={ref} role="dialog" aria-label={label}>
      {children}
    </div>
  );
}

/** A picker's field, which has the keyboard while the picker is up: what's typed filters its
 *  list or names something new, Return takes it, and Escape closes the picker. */
function PickerFilter({ value, placeholder, onChange, onEnter, onClose }: { value: string; placeholder: string; onChange: (value: string) => void; onEnter: () => void; onClose: () => void }) {
  return (
    <input
      className="ca-labelpick-input"
      value={value}
      autoFocus
      placeholder={placeholder}
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          e.stopPropagation();
          onClose();
        } else if (e.key === 'Enter') {
          e.preventDefault();
          onEnter();
        }
      }}
    />
  );
}

interface LabelPickerProps {
  labels: Label[];
  chosen: string[];
  onToggle: (id: string) => void;
  onCreate: (name: string) => void;
  onRename: (id: string, name: string) => void;
  /** Right-click on a label: its color, rename, delete. */
  onMenu: (label: Label, rename: () => void) => void;
  onClose: () => void;
}

/** Pick labels, or type a new one and press Return. Right-click a label to recolor, rename or
 *  delete it. */
function LabelPicker({ labels, chosen, onToggle, onCreate, onRename, onMenu, onClose }: LabelPickerProps) {
  const [query, setQuery] = useState('');
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  const q = query.trim().toLowerCase();
  const shown = labels.filter((l) => l.name.toLowerCase().includes(q));
  const exact = labels.find((l) => l.name.toLowerCase() === q);
  return (
    <PickerBox label="Labels" onClose={onClose}>
      <PickerFilter
        value={query}
        placeholder={labels.length ? 'Filter, or type a new label' : 'Type a label'}
        onChange={setQuery}
        onEnter={() => {
          if (exact) onToggle(exact.id);
          else if (q) onCreate(query.trim());
          setQuery('');
        }}
        onClose={onClose}
      />
      <div className="ca-labelpick-list">
        {shown.map((l) =>
          renaming?.id === l.id ? (
            <div key={l.id} className="ca-menu-item is-renaming">
              <LabelDot label={l} />
              <input
                className="ca-labelpick-rename"
                value={renaming.name}
                autoFocus
                onChange={(e) => setRenaming({ id: l.id, name: e.target.value })}
                onBlur={() => {
                  if (renaming.name.trim()) onRename(l.id, renaming.name.trim());
                  setRenaming(null);
                }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') e.currentTarget.blur();
                  if (e.key === 'Escape') {
                    e.preventDefault();
                    e.stopPropagation();
                    setRenaming(null);
                  }
                }}
              />
            </div>
          ) : (
            <button
              key={l.id}
              type="button"
              role="menuitemcheckbox"
              aria-checked={chosen.includes(l.id)}
              className="ca-menu-item"
              onClick={() => onToggle(l.id)}
              onContextMenu={(e) => {
                e.preventDefault();
                onMenu(l, () => setRenaming({ id: l.id, name: l.name }));
              }}
            >
              <span className="ca-labelpick-check">{chosen.includes(l.id) && <Icon name="check" size={14} stroke={2} />}</span>
              <LabelDot label={l} />
              <span className="ca-menu-label">{l.name}</span>
            </button>
          ),
        )}
        {q && !exact && (
          <button
            type="button"
            className="ca-menu-item"
            onClick={() => {
              onCreate(query.trim());
              setQuery('');
            }}
          >
            <span className="ca-labelpick-check">
              <Icon name="plus" size={14} />
            </span>
            <span className="ca-menu-label">Create “{query.trim()}”</span>
          </button>
        )}
        {!q && labels.length === 0 && <p className="ca-labelpick-note">No labels yet. Type one to make it.</p>}
      </div>
      {labels.length > 0 && <p className="ca-labelpick-note">Right-click a label to change its color, rename or delete it.</p>}
    </PickerBox>
  );
}

/** Pick a todo for this one to wait on, or type what else it waits for: a person, a release.
 *  Done and archived ones and this one aren't offered; ones that already wait on this one are shown greyed,
 *  since waiting on them would make a loop. */
function WaitPicker({ todo, todos, onPick, onWaitFor, onClose }: { todo: Todo; todos: Todo[]; onPick: (id: string) => void; onWaitFor: (note: string) => void; onClose: () => void }) {
  const [query, setQuery] = useState('');
  const q = query.trim().toLowerCase();
  const offered = todos
    .filter((t) => t.id !== todo.id && isOpen(t) && !depsOf(todo).includes(t.id))
    .map((t) => ({ t, why: cantWaitOn(todo, t, todos) }))
    .filter(({ t }) => (t.title || 'Untitled').toLowerCase().includes(q))
    // The todo's own project first; the rest keep your order.
    .sort((a, b) => Number(a.t.project !== todo.project) - Number(b.t.project !== todo.project));
  const first = offered.find((o) => !o.why);
  // A done todo can't be marked as waiting.
  const note = todo.status !== 'done' ? query.replace(/\s+/g, ' ').trim() : '';
  return (
    <PickerBox label="Waits on" className="ca-waitpick" onClose={onClose}>
      <PickerFilter
        value={query}
        placeholder={todo.status !== 'done' ? 'Find a todo, or say what else' : 'Find a todo'}
        onChange={setQuery}
        onEnter={() => {
          if (first) {
            onPick(first.t.id);
            setQuery('');
          } else if (note) {
            onWaitFor(note);
            onClose();
          }
        }}
        onClose={onClose}
      />
      <div className="ca-labelpick-list">
        {offered.map(({ t, why }) => (
          <button
            key={t.id}
            type="button"
            className="ca-menu-item"
            disabled={!!why}
            title={why ? `${why}: waiting on it would make a loop.` : undefined}
            onClick={() => {
              onPick(t.id);
              setQuery('');
            }}
          >
            <TodoGlyph status={t.status} />
            <span className="ca-menu-label">{t.title || 'Untitled'}</span>
            {why ? <span className="ca-waitpick-note">{why}</span> : t.project !== todo.project && t.project ? <span className="ca-waitpick-note">{t.project}</span> : null}
          </button>
        ))}
        {offered.length === 0 && <p className="ca-labelpick-note">{q ? 'No open todo matches.' : 'No other open todos.'}</p>}
        {note && (
          <button
            type="button"
            className="ca-menu-item"
            onClick={() => {
              onWaitFor(note);
              onClose();
            }}
          >
            <Icon name="hourglass" size={14} />
            <span className="ca-menu-label">Wait for “{note}”</span>
          </button>
        )}
      </div>
      {todo.status !== 'done' && !note && <p className="ca-labelpick-note">Or type what else it waits for: a person’s answer, a release, a date.</p>}
    </PickerBox>
  );
}

/** Pick the day a todo is due: today, tomorrow, the coming Friday or Monday, or any day in the
 *  date field, which takes the keyboard (⏎ sets it). */
function DuePicker({ due, now, onPick, onClose }: { due: string | null; now: number; onPick: (due: string | null) => void; onClose: () => void }) {
  const [day, setDay] = useState(due ?? dayKey(now));
  const pick = (d: string | null) => {
    onPick(d);
    onClose();
  };
  return (
    <PickerBox label="Due date" className="ca-duepick" onClose={onClose}>
      <form
        className="ca-duepick-day"
        onSubmit={(e) => {
          e.preventDefault();
          if (dueDate(day)) pick(day);
        }}
      >
        <input
          type="date"
          className="ca-labelpick-input"
          aria-label="Due on"
          value={day}
          autoFocus
          onChange={(e) => setDay(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              e.preventDefault();
              e.stopPropagation();
              onClose();
            }
          }}
        />
        <Button type="submit" size="sm" variant="primary" disabled={!dueDate(day) || day === due}>
          Set
        </Button>
      </form>
      <div className="ca-labelpick-list">
        {dueChoices(now).map((c) => (
          <button key={c.due} type="button" role="menuitemradio" aria-checked={due === c.due} className="ca-menu-item" onClick={() => pick(c.due)}>
            <span className="ca-labelpick-check">{due === c.due && <Icon name="check" size={14} stroke={2} />}</span>
            <span className="ca-menu-label">{c.label}</span>
            <span className="ca-waitpick-note">{dueDate(c.due)?.toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' })}</span>
          </button>
        ))}
        {due && (
          <button type="button" className="ca-menu-item" onClick={() => pick(null)}>
            <span className="ca-labelpick-check">
              <Icon name="x" size={12} stroke={2} />
            </span>
            <span className="ca-menu-label">Remove due date</span>
          </button>
        )}
      </div>
    </PickerBox>
  );
}

/** Another todo, as the Waits on and Needed by lines show it. Click it to show it. */
function TodoRef({ todo, onShow, onRemove }: { todo: Todo; onShow: (t: Todo) => void; onRemove?: () => void }) {
  const title = todo.title || 'Untitled';
  return (
    <Chip kind="label" className={`ca-todoref is-${todo.status}`} remove={onRemove && { label: `Stop waiting on ${title}`, title: 'Stop waiting on it', run: onRemove }}>
      <button type="button" className="ca-todoref-show" title={`${STATUS_WORD[todo.status]}: ${title}. Click to show it.`} onClick={() => onShow(todo)}>
        <TodoGlyph status={todo.status} size={11} />
        <span className="ca-todoref-title">{title}</span>
      </button>
    </Chip>
  );
}

/** What a linked worktree says of itself: its PR, else its branch when that says more. */
function placeNote(lane: Lane): string | null {
  const branch = branchLabel(lane);
  if (lane.pr) return `#${lane.pr.number}`;
  return branch && branch !== lane.callsign ? middleTruncate(branch, 28) : null;
}

interface TodoPanelProps {
  item: TodoItem;
  /** Every todo, in any project, for what this one waits on and what waits on it. */
  todos: Todo[];
  /** Shows another todo, from Waits on or Needed by. */
  onShowTodo: (todo: Todo) => void;
  /** Every label, in your order. */
  labels: Label[];
  lanes: Map<string, Lane>;
  now: number;
  onRun: (op: TodoOp) => void;
  /** Opens the new-session sheet for this todo. */
  onStart: () => void;
  /** Shows a session in the panel, in place of the todo until you go back. */
  onShowSession: (lane: Lane, session: Session) => void;
  /** Opens a session where it runs. */
  onOpenSession: (lane: Lane, session: Session) => void;
  onShowLane: (lane: Lane) => void;
  /** Right-click on a linked worktree; `linked` when it's linked itself, not through a session. */
  onLaneMenu: (lane: Lane, linked: boolean) => void;
  onSessionMenu: (lane: Lane, session: Session) => void;
  /** Link…: a menu of worktrees and sessions, dropped from the button. */
  onLinkMenu: (at: { x: number; y: number }) => void;
  /** The ⋯ button: move, copy, delete. */
  onMoreMenu: (at: { x: number; y: number }) => void;
  /** Right-click on its header, the title too unless you're typing in it: the todo's menu, as
   *  its row has it. */
  onMenu?: () => void;
  /** Right-click on a label in the picker. */
  onLabelMenu: (label: Label, rename: () => void) => void;
  onError: (message: string) => void;
  /** Jev says its work looks done. */
  hint?: TodoHint | null;
  onDismissHint?: () => void;
  /** `[jev] todos` is on: ask Jev for its project, labels and what it waits on. */
  suggest?: boolean;
  /** The projects a todo can be in. */
  projects?: string[];
  /** Open the due date's picker, as Pick a date… in its menu and D in the list ask. */
  pickDue?: boolean;
  /** The picker opened for `pickDue`. */
  onPickDue?: () => void;
}

export function TodoPanel({ item, todos, onShowTodo, labels, lanes, now, onRun, onStart, onShowSession, onOpenSession, onShowLane, onLaneMenu, onSessionMenu, onLinkMenu, onMoreMenu, onMenu, onLabelMenu, onError, hint, onDismissHint, suggest = false, projects = [], pickDue = false, onPickDue }: TodoPanelProps) {
  const { todo } = item;
  const section = itemSection(item, todos);
  const [picking, setPicking] = useState(false);
  const [pickingWait, setPickingWait] = useState(false);
  const [pickingDue, setPickingDue] = useState(false);
  useEffect(() => {
    if (!pickDue) return;
    setPickingDue(true);
    onPickDue?.();
  }, [pickDue, onPickDue]);
  const due = todo.due ? dueWords(todo.due, now) : null;
  const waitsOn = waitsOnTodos(todo, todos);
  const neededBy = dependents(todo, todos);
  const setDeps = (dependsOn: string[]) => update({ dependsOn });
  const update = (patch: Extract<TodoOp, { op: 'update' }>['patch']) => onRun({ op: 'update', id: todo.id, patch });
  const toggleLabel = (id: string) => update({ labels: todo.labels.includes(id) ? todo.labels.filter((l) => l !== id) : [...todo.labels, id] });
  const shownLabels = todo.labels.flatMap((id) => labels.find((l) => l.id === id) ?? []);
  const waiting = todo.pending.map((p) => ({ p, lane: lanes.get(p.lane) }));
  const suggested = useSuggested(todo, suggest);
  const offer = suggested ? todoSuggestions(suggested, todo, labels, todos, projects) : null;
  // Its worktrees and sessions as chips, or with their details as a tree, as you left them.
  const [placesOpen, togglePlaces] = useDetailsOpen(PLACES_KEY);

  return (
    <aside className="ca-insp ca-todoinsp" aria-label="Todo details">
      <header onContextMenu={headerMenu(onMenu)}>
        <div className="ca-insp-title">
          <TodoGlyph status={todo.status} waiting={section === 'waiting'} review={section === 'review'} size={14} />
          <span className="ca-tag">{SECTION_WORD[section]}</span>
          {todo.project && <span className="ca-tag">{todo.project}</span>}
          {offer?.project && (
            <button type="button" className="ca-tag ca-suggestchip" title="Suggested by Jev. Click to move it there." onClick={() => update({ project: offer.project ?? undefined })}>
              <Icon name="sparkles" size={11} />
              {todo.project ? `Move to ${offer.project}?` : `In ${offer.project}?`}
            </button>
          )}
          <Button variant="ghost" size="sm" icon="dots" aria-label="More" title="Move, copy, archive or delete" className="ca-todoinsp-more" onClick={(e) => onMoreMenu(below(e.currentTarget))} />
        </div>
        <TitleField value={todo.title} onSave={(title) => update({ title })} />
        <div className="ca-todoinsp-meta">
          <span title={absolute(todo.createdAt)}>Added {ago(todo.createdAt, now)} ago</span>
          {todo.doneAt ? <span title={absolute(todo.doneAt)}>done {ago(todo.doneAt, now)} ago</span> : todo.updatedAt > todo.createdAt ? <span title={absolute(todo.updatedAt)}>changed {ago(todo.updatedAt, now)} ago</span> : null}
        </div>
      </header>
      <div className="ca-insp-actions ca-todoinsp-actions">
        {item.primary ? (
          <>
            {/* Its primary session runs: what you want done goes there, so it comes first. */}
            <Button variant="primary" icon="message-circle" title={`Show “${item.primary.session.title}”, its primary session, here`} onClick={() => item.primary && onShowSession(item.primary.lane, item.primary.session)}>
              Show session
            </Button>
            <Button icon="player-play" title={keysIn(`Start another session for this todo (⌘N). It works alongside “${item.primary.session.title}”, which stays its primary session.`)} onClick={onStart}>
              Start another
            </Button>
          </>
        ) : (
          <Button variant="primary" icon="player-play" title={keysIn('Start a session for this todo (⌘N). ⌘-click other todos in the list to start them with it, in one session.')} onClick={onStart}>
            Start session
          </Button>
        )}
        <Seg label="Status" options={STATUSES.map((st) => ({ id: st, label: STATUS_WORD[st] }))} value={todo.status} onPick={(status) => update({ status })} />
      </div>

      {hint && todo.status !== 'done' && (
        <div className="ca-todohint" role="status">
          <Icon name="check" size={14} />
          <span className="ca-todohint-text">
            <b>Looks done</b>
            <span title={`Jev is ${Math.round(hint.p * 100)}% sure`}>{hint.why}</span>
          </span>
          <Button size="sm" onClick={() => update({ status: 'done' })}>
            Mark done
          </Button>
          <Button variant="ghost" size="sm" title="Asks again once something linked to it moves on" onClick={onDismissHint}>
            Not yet
          </Button>
        </div>
      )}

      <section className="ca-insp-sec">
        <h4 className="ca-label">Labels</h4>
        <div className="ca-todoinsp-labels">
          {shownLabels.map((l) => (
            <LabelChip key={l.id} label={l} remove={() => toggleLabel(l.id)} />
          ))}
          {offer?.labels.map((l) => (
            <LabelChip key={l.id} label={l} suggested title={`Suggested by Jev. Click to add “${l.name}”.`} onClick={() => toggleLabel(l.id)} />
          ))}
          <span className="ca-labelpick-anchor">
            <AddChip icon="tag" label={shownLabels.length ? 'Label' : 'Add label'} open={picking} onToggle={() => setPicking(!picking)} />
            {picking && (
              <LabelPicker
                labels={labels}
                chosen={todo.labels}
                onToggle={toggleLabel}
                onCreate={(name) => {
                  const label: Label = { id: newId('l'), name, color: nextColor(labels) };
                  onRun({ op: 'labels', labels: [...labels, label] });
                  update({ labels: [...todo.labels, label.id] });
                }}
                onRename={(id, name) => onRun({ op: 'labels', labels: labels.map((l) => (l.id === id ? { ...l, name } : l)) })}
                onMenu={onLabelMenu}
                onClose={() => setPicking(false)}
              />
            )}
          </span>
        </div>
      </section>

      <section className="ca-insp-sec">
        <h4 className="ca-label">Due</h4>
        <div className="ca-todoinsp-labels">
          <span className="ca-labelpick-anchor ca-duepick-anchor">
            {due ? (
              <Chip kind="label" className={cx('ca-duenote', todo.status !== 'done' && `is-${due.tone}`)} remove={{ label: 'Remove the due date', title: 'Remove the due date', run: () => update({ due: null }) }}>
                <button type="button" className="ca-duenote-show" aria-expanded={pickingDue} title="Change the due date (D)" onClick={() => setPickingDue(!pickingDue)}>
                  <Icon name="calendar" size={12} />
                  <span>{due.long}</span>
                </button>
              </Chip>
            ) : (
              <AddChip icon="calendar" label="Add due date" title="Add a due date (D)" open={pickingDue} onToggle={() => setPickingDue(!pickingDue)} />
            )}
            {pickingDue && <DuePicker due={todo.due ?? null} now={now} onPick={(d) => update({ due: d })} onClose={() => setPickingDue(false)} />}
          </span>
        </div>
      </section>

      <section className="ca-insp-sec">
        <h4 className="ca-label">Waits on</h4>
        <div className="ca-todoinsp-labels ca-todoinsp-waits">
          {waitsOn.map((t) => (
            <TodoRef key={t.id} todo={t} onShow={onShowTodo} onRemove={() => setDeps(depsOf(todo).filter((d) => d !== t.id))} />
          ))}
          {todo.waiting && (
            <Chip
              kind="label"
              className="ca-waitnote"
              title={`Marked as waiting${todo.waiting.since > 0 ? ` ${ago(todo.waiting.since, now)} ago` : ''}. It stays under Waiting until you say it’s ready, start it, or finish it.`}
              remove={{ label: 'Stop waiting', title: 'Stop waiting', run: () => update({ waiting: null }) }}
            >
              <Icon name="hourglass" size={12} />
              <span className="ca-waitnote-text">{todo.waiting.note || 'Marked as waiting'}</span>
            </Chip>
          )}
          {offer?.dependsOn.map((t) => (
            <Chip key={t.id} kind="label" className="ca-suggestchip" title={`Suggested by Jev: it may need “${t.title || 'Untitled'}” done first. Click to wait on it.`} act={() => setDeps([...depsOf(todo), t.id])}>
              <Icon name="sparkles" size={11} />
              <span className="ca-waitnote-text">{middleTruncate(t.title || 'Untitled', 40)}?</span>
            </Chip>
          ))}
          <span className="ca-labelpick-anchor ca-waitpick-anchor">
            <AddChip icon="lock" label={waitsOn.length || todo.waiting ? 'Add' : 'Add what it waits on'} open={pickingWait} onToggle={() => setPickingWait(!pickingWait)} />
            {pickingWait && (
              <WaitPicker
                todo={todo}
                todos={todos}
                onPick={(id) => setDeps([...depsOf(todo), id])}
                onWaitFor={(note) => update({ waiting: { note, since: 0 } })}
                onClose={() => setPickingWait(false)}
              />
            )}
          </span>
        </div>
        {neededBy.length > 0 && (
          <div className="ca-todoinsp-neededby">
            <span className="ca-todoinsp-neededby-head">Needed by</span>
            {neededBy.map((t) => (
              <TodoRef key={t.id} todo={t} onShow={onShowTodo} />
            ))}
          </div>
        )}
      </section>

      <section className="ca-insp-sec">
        <h4 className="ca-label">Description</h4>
        <DescriptionEditor key={todo.id} value={todo.description} onSave={(description) => update({ description })} onError={onError} />
      </section>

      <section className="ca-insp-sec">
        <h4 className="ca-label ca-todoinsp-linkhead">
          <span>Worktrees and sessions</span>
          {item.places.length > 0 && <DetailsButton open={placesOpen} says="each session’s title and what it’s doing" onToggle={togglePlaces} className="ca-todoinsp-details" />}
          <button type="button" className="ca-linkbtn" onClick={(e) => onLinkMenu(below(e.currentTarget))}>
            <Icon name="link" size={12} />
            Link
          </button>
        </h4>
        {item.places.length > 0 && (
          <PanelPlaces
            places={item.places.map((p) => ({ lane: p.lane, sessions: p.sessions, note: placeNote(p.lane) }))}
            open={placesOpen}
            label="Worktrees and sessions"
            now={now}
            onShowLane={onShowLane}
            onLaneMenu={(lane) => onLaneMenu(lane, !!item.places.find((p) => p.lane.id === lane.id)?.linked)}
            onShowSession={onShowSession}
            onOpenSession={onOpenSession}
            onSessionMenu={onSessionMenu}
            primary={item.primary && { key: item.primary.session.key, means: TODO_PRIMARY }}
          />
        )}
        {waiting.map(({ p, lane }) => (
          <p key={`${p.lane}@${p.at}`} className="ca-todoinsp-waiting">
            <Icon name="clock" size={12} />
            <span>
              Waiting for the new {p.harness === 'codex' ? 'Codex thread' : p.harness === 'opencode' ? 'OpenCode session' : 'Claude session'} in {lane?.callsign ?? 'its worktree'} to show up. Started {ago(p.at, now)} ago.
            </span>
          </p>
        ))}
        {item.places.length === 0 && waiting.length === 0 && (
          <p className="ca-empty">
            Nothing yet. Start a session from this todo and it’s linked here, or link a worktree or session you already have. <Kbd keys={['⌘', 'N']} /> starts one.
          </p>
        )}
      </section>
    </aside>
  );
}

interface TodoPickPanelProps {
  /** The todos picked, in the list's order, which is how the prompt numbers them. */
  items: TodoItem[];
  /** Every todo, in any project, for what each waits on. */
  todos: Todo[];
  /** Show each one's project: the view spans all of them. */
  showProject: boolean;
  /** Opens the quick panel for one session on all of them. */
  onStart: () => void;
  /** Selects only this one. */
  onShow: (item: TodoItem) => void;
  /** Takes it out of the ones picked. */
  onLeaveOut: (item: TodoItem) => void;
  /** The ⋯ button and a right-click on the header: status, copy as prompt, delete. */
  onMoreMenu: (at?: { x: number; y: number }) => void;
}

/** Several todos picked in the list (⌘-click, ⇧-click), in the pick panel (PickPanel.tsx): one
 *  session for all of them, started on one prompt that has each, numbered as listed here, and
 *  linked to each. */
export function TodoPickPanel({ items, todos, showProject, onStart, onShow, onLeaveOut, onMoreMenu }: TodoPickPanelProps) {
  const ids = items.map((r) => r.todo.id);
  return (
    <PickPanel
      label="Selected todos"
      icon="list-check"
      head={`${items.length} todos`}
      help="⌘-click adds a todo or leaves it out, ⇧-click adds a run of them. Esc goes back to one."
      moreTitle="Status, copy as prompt or IDs, delete"
      onMoreMenu={onMoreMenu}
      actions={
        <>
          <div className="ca-insp-actions ca-todoinsp-actions">
            <Button variant="primary" icon="player-play" title={keysIn('Start one session for all of them (⌘N)')} onClick={onStart}>
              Start one session
            </Button>
          </div>
          <p className="ca-todopick-note">The session starts on one prompt with each todo in it, numbered as below, and it’s linked to all {items.length}.</p>
        </>
      }
    >
      <PickList label="In the prompt" numbered>
        {items.map((r, i) => {
          const t = r.todo;
          const title = t.title || 'Untitled';
          // What it waits on outside the ones picked: those are done together.
          const open = t.status === 'done' ? [] : blockedBy(t, todos).filter((w) => !ids.includes(w.id));
          return (
            <PickRow key={r.key} n={i + 1} lead={<TodoGlyph status={t.status} waiting={sectionOf(t, todos) === 'waiting'} size={12} />} name={title} title={`${title}. Click to show only this todo.`} onShow={() => onShow(r)} onLeaveOut={() => onLeaveOut(r)}>
              {showProject && t.project && <span className="ca-tag">{t.project}</span>}
              {open.length > 0 && (
                <span className="ca-todopick-flag" title={`Waits on ${open.map((w) => `“${w.title || 'Untitled'}”`).join(', ')}, not done yet.`}>
                  <Icon name="lock" size={12} />
                </span>
              )}
              {t.waiting && t.status !== 'done' && (
                <span className="ca-todopick-flag" title={`${t.waiting.note ? `Waiting for ${t.waiting.note}` : 'Marked as waiting'}. Starting a session on it ends the wait.`}>
                  <Icon name="hourglass" size={12} />
                </span>
              )}
              {r.sessions.length > 0 && (
                <span className="ca-todopick-sessions" title={r.sessions.map(({ session }) => `${session.account} · ${session.title}`).join('\n')}>
                  {r.sessions.length === 1 ? '1 session' : `${r.sessions.length} sessions`}
                </span>
              )}
            </PickRow>
          );
        })}
      </PickList>
    </PickPanel>
  );
}
