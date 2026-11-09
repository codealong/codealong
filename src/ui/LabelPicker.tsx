// Picking labels: the box a picker drops in under the chip that opens it, its filter, and the
// labels in it to put on or take off, or a new one typed. A todo's panel picks its labels with it
// (TodoPanel.tsx), and a session's header its own (SessionHeader.tsx `SessionLabels`); the todo
// panel's other pickers (what it waits on, when it's due) drop in the same box.

import { useRef, useState, type ReactNode } from 'react';
import type { Label } from '../types';
import { cx, Icon } from './primitives';
import { LabelDot } from './TodoBits';
import { useDismiss } from './useDismiss';

/** A picker under the chip that opens it: it goes on a press anywhere else and on Escape; a
 *  press on its own chip, beside it in the same anchor, is the chip's, which closes it itself. */
export function PickerBox({ label, className, onClose, children }: { label: string; className?: string; onClose: () => void; children: ReactNode }) {
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
export function PickerFilter({ value, placeholder, onChange, onEnter, onClose }: { value: string; placeholder: string; onChange: (value: string) => void; onEnter: () => void; onClose: () => void }) {
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

export interface LabelPickerProps {
  labels: Label[];
  chosen: string[];
  onToggle: (id: string) => void;
  onCreate: (name: string) => void;
  /** Renames a label, from its right-click menu (`onMenu`). */
  onRename?: (id: string, name: string) => void;
  /** Right-click on a label: its color, rename, delete. Without it a right-click does nothing. */
  onMenu?: (label: Label, rename: () => void) => void;
  onClose: () => void;
}

/** Pick labels, or type a new one and press Return. Right-click a label to recolor, rename or
 *  delete it, where the window says how (`onMenu`). */
export function LabelPicker({ labels, chosen, onToggle, onCreate, onRename, onMenu, onClose }: LabelPickerProps) {
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
                  if (renaming.name.trim()) onRename?.(l.id, renaming.name.trim());
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
                onMenu?.(l, () => setRenaming({ id: l.id, name: l.name }));
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
      {labels.length > 0 && onMenu && <p className="ca-labelpick-note">Right-click a label to change its color, rename or delete it.</p>}
    </PickerBox>
  );
}
