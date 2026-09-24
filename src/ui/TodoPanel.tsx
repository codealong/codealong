// A todo in the right panel: its title, status and labels, the description with its
// screenshots, and the worktrees and sessions it's linked to, drawn like the sidebar's tree.
// Everything saves as you go. Start session opens the new-session sheet with the todo as the
// prompt, and the session it starts is linked to the todo.

import { useEffect, useRef, useState } from 'react';
import { below } from '../contextMenu';
import { absolute, ago, middleTruncate } from '../format';
import { newId, nextColor, STATUSES, STATUS_WORD, type TodoItem, type TodoOp, type TodoPlace } from '../todos';
import type { Label, Lane, Session } from '../types';
import { DescriptionEditor } from './DescriptionEditor';
import { Button, Icon, Kbd, LaneGlyph, StatusGlyph } from './primitives';
import { LabelDot, TodoGlyph } from './TodoBits';

/** The title, as a heading you can type in. Return or leaving it saves. */
function TitleField({ value, onSave }: { value: string; onSave: (title: string) => void }) {
  const [draft, setDraft] = useState(value);
  const [focused, setFocused] = useState(false);
  const ref = useRef<HTMLTextAreaElement>(null);
  // The latest save, so a new board (and with it a new callback) doesn't restart the wait.
  const save = useRef(onSave);
  save.current = onSave;
  useEffect(() => {
    if (!focused) setDraft(value);
  }, [value, focused]);
  useEffect(() => {
    if (!focused || draft.trim() === value.trim()) return;
    const id = window.setTimeout(() => save.current(draft), 600);
    return () => window.clearTimeout(id);
  }, [draft, focused, value]);
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
      onFocus={() => setFocused(true)}
      onChange={(e) => setDraft(e.target.value.replace(/\n/g, ' '))}
      onBlur={() => {
        setFocused(false);
        if (draft.trim() !== value.trim()) onSave(draft);
      }}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === 'Escape') {
          e.preventDefault();
          e.stopPropagation();
          if (e.key === 'Escape') setDraft(value);
          ref.current?.blur();
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
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [onClose]);
  const q = query.trim().toLowerCase();
  const shown = labels.filter((l) => l.name.toLowerCase().includes(q));
  const exact = labels.find((l) => l.name.toLowerCase() === q);
  return (
    <div className="ca-menu ca-labelpick" ref={ref} role="dialog" aria-label="Labels">
      <input
        className="ca-labelpick-input"
        value={query}
        autoFocus
        placeholder={labels.length ? 'Filter, or type a new label' : 'Type a label'}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            e.stopPropagation();
            onClose();
          } else if (e.key === 'Enter') {
            e.preventDefault();
            if (exact) onToggle(exact.id);
            else if (q) onCreate(query.trim());
            setQuery('');
          }
        }}
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
    </div>
  );
}

/** A worktree the todo is linked to, and its sessions, as the sidebar's tree draws them. */
function Place({ place, now, onShowLane, onLaneMenu, onOpenSession, onSessionMenu }: { place: TodoPlace; now: number } & Pick<TodoPanelProps, 'onShowLane' | 'onLaneMenu' | 'onOpenSession' | 'onSessionMenu'>) {
  const { lane, sessions } = place;
  const branch = lane.branch ?? (lane.detached ? `detached @ ${lane.detached}` : null);
  return (
    <div role="treeitem" aria-expanded={sessions.length > 0 ? true : undefined}>
      <div
        className="ca-tree-node"
        onContextMenu={(e) => {
          e.preventDefault();
          onLaneMenu(lane, place.linked);
        }}
      >
        <button type="button" className="ca-tree-label" title={`${lane.path}. Click to show it in Worktrees.`} onClick={() => onShowLane(lane)}>
          <LaneGlyph lane={lane} />
          <span className="ca-nav-label">{lane.callsign}</span>
          {lane.pr ? <span className="ca-tree-note">#{lane.pr.number}</span> : branch && branch !== lane.callsign ? <span className="ca-tree-note">{middleTruncate(branch, 28)}</span> : null}
        </button>
      </div>
      {sessions.length > 0 && (
        <div className="ca-tree-children" role="group">
          {sessions.map((s) => (
            <button
              key={s.key}
              type="button"
              role="treeitem"
              className="ca-tree-session"
              title={`${s.account} · ${s.title}. Click to open it where it lives.`}
              onClick={() => onOpenSession(lane, s)}
              onContextMenu={(e) => {
                e.preventDefault();
                onSessionMenu(lane, s);
              }}
            >
              <StatusGlyph status={s.status} size={10} />
              <span className="ca-tree-acct">{s.account}</span>
              <span className="ca-tree-title">{s.title}</span>
              <span className="ca-tree-age" title={absolute(s.lastActivity)}>
                {ago(s.lastActivity, now)}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

interface TodoPanelProps {
  item: TodoItem;
  /** Every label, in your order. */
  labels: Label[];
  lanes: Map<string, Lane>;
  now: number;
  onRun: (op: TodoOp) => void;
  /** Opens the new-session sheet for this todo. */
  onStart: () => void;
  onOpenSession: (lane: Lane, session: Session) => void;
  onShowLane: (lane: Lane) => void;
  /** Right-click on a linked worktree; `linked` when it's linked itself, not through a session. */
  onLaneMenu: (lane: Lane, linked: boolean) => void;
  onSessionMenu: (lane: Lane, session: Session) => void;
  /** Link…: a menu of worktrees and sessions, dropped from the button. */
  onLinkMenu: (at: { x: number; y: number }) => void;
  /** The ⋯ button: move, copy, delete. */
  onMoreMenu: (at: { x: number; y: number }) => void;
  /** Right-click on a label in the picker. */
  onLabelMenu: (label: Label, rename: () => void) => void;
  onError: (message: string) => void;
}

export function TodoPanel({ item, labels, lanes, now, onRun, onStart, onOpenSession, onShowLane, onLaneMenu, onSessionMenu, onLinkMenu, onMoreMenu, onLabelMenu, onError }: TodoPanelProps) {
  const { todo } = item;
  const [picking, setPicking] = useState(false);
  const update = (patch: Extract<TodoOp, { op: 'update' }>['patch']) => onRun({ op: 'update', id: todo.id, patch });
  const toggleLabel = (id: string) => update({ labels: todo.labels.includes(id) ? todo.labels.filter((l) => l !== id) : [...todo.labels, id] });
  const shownLabels = todo.labels.flatMap((id) => labels.find((l) => l.id === id) ?? []);
  const waiting = todo.pending.map((p) => ({ p, lane: lanes.get(p.lane) }));

  return (
    <aside className="ca-insp ca-todoinsp" aria-label="Todo details">
      <header>
        <div className="ca-insp-title">
          <TodoGlyph status={todo.status} size={14} />
          <span className="ca-tag">{STATUS_WORD[todo.status]}</span>
          {todo.project && <span className="ca-tag">{todo.project}</span>}
          <Button variant="ghost" size="sm" icon="dots" aria-label="More" title="Move, copy or delete" className="ca-todoinsp-more" onClick={(e) => onMoreMenu(below(e.currentTarget))} />
        </div>
        <TitleField value={todo.title} onSave={(title) => update({ title })} />
        <div className="ca-todoinsp-meta">
          <span title={absolute(todo.createdAt)}>Added {ago(todo.createdAt, now)} ago</span>
          {todo.doneAt ? <span title={absolute(todo.doneAt)}>done {ago(todo.doneAt, now)} ago</span> : todo.updatedAt > todo.createdAt ? <span title={absolute(todo.updatedAt)}>changed {ago(todo.updatedAt, now)} ago</span> : null}
        </div>
      </header>
      <div className="ca-insp-actions ca-todoinsp-actions">
        <Button variant="primary" icon="player-play" title="Start a session for this todo (⌘N)" onClick={onStart}>
          Start session
        </Button>
        <div className="ca-segctl" role="radiogroup" aria-label="Status">
          {STATUSES.map((st) => (
            <button key={st} type="button" role="radio" aria-checked={todo.status === st} className={todo.status === st ? 'is-on' : undefined} onClick={() => update({ status: st })}>
              {STATUS_WORD[st]}
            </button>
          ))}
        </div>
      </div>

      <section className="ca-insp-sec">
        <h4 className="ca-label">Labels</h4>
        <div className="ca-todoinsp-labels">
          {shownLabels.map((l) => (
            <span key={l.id} className="ca-labelchip">
              <LabelDot label={l} />
              <span>{l.name}</span>
              <button type="button" aria-label={`Remove ${l.name}`} title={`Remove ${l.name}`} onClick={() => toggleLabel(l.id)}>
                <Icon name="x" size={10} stroke={2.25} />
              </button>
            </span>
          ))}
          <span className="ca-labelpick-anchor">
            <button type="button" className="ca-labelchip ca-labelchip--add" aria-expanded={picking} onClick={() => setPicking(!picking)}>
              <Icon name="tag" size={12} />
              <span>{shownLabels.length ? 'Label' : 'Add label'}</span>
            </button>
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
        <h4 className="ca-label">Description</h4>
        <DescriptionEditor key={todo.id} value={todo.description} onSave={(description) => update({ description })} onError={onError} />
      </section>

      <section className="ca-insp-sec">
        <h4 className="ca-label ca-todoinsp-linkhead">
          <span>Worktrees and sessions</span>
          <button type="button" className="ca-linkbtn" onClick={(e) => onLinkMenu(below(e.currentTarget))}>
            <Icon name="link" size={12} />
            Link
          </button>
        </h4>
        {item.places.length > 0 && (
          <div className="ca-tree" role="tree" aria-label="Worktrees and sessions">
            {item.places.map((place) => (
              <Place key={place.lane.id} place={place} now={now} onShowLane={onShowLane} onLaneMenu={onLaneMenu} onOpenSession={onOpenSession} onSessionMenu={onSessionMenu} />
            ))}
          </div>
        )}
        {waiting.map(({ p, lane }) => (
          <p key={`${p.lane}@${p.at}`} className="ca-todoinsp-waiting">
            <Icon name="clock" size={12} />
            <span>
              Waiting for the new {p.harness === 'codex' ? 'Codex thread' : 'Claude app session'} in {lane?.callsign ?? 'its worktree'} to show up. Started {ago(p.at, now)} ago.
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
