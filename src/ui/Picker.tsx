// A menu that drops from a button in the quick panel: a filter field, then the options in
// sections, and below them a setting that goes with whatever is picked (the agent menu's effort).
// It's made for the keyboard: typing filters, ↑ ↓ move, ⏎ picks, ← → change the setting while
// the filter is empty, and Escape closes it and leaves the panel open. Native menus aren't used here: the panel doesn't activate
// Codealong, and a menu can't show usage meters or filter.

import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useDismiss } from './useDismiss';
import { cx } from './primitives';

export interface PickerOption {
  id: string;
  /** What the filter matches, lowercased or not. */
  text: string;
  label: ReactNode;
  detail?: ReactNode;
  /** Shown at the start of the row. */
  lead?: ReactNode;
  /** Shown at the end of the row. */
  trail?: ReactNode;
  disabled?: boolean;
  title?: string;
}

export interface PickerSection {
  key: string;
  head?: ReactNode;
  options: PickerOption[];
}

interface PickerProps {
  sections: PickerSection[];
  value: string | null;
  placeholder: string;
  onPick: (id: string) => void;
  onClose: () => void;
  /** Said when the filter leaves nothing. */
  empty?: string;
  className?: string;
  /** A setting under the options, which stays put while they're filtered. */
  foot?: ReactNode;
  /** ← or → while the filter is empty, for the setting below. */
  onStep?: (delta: -1 | 1) => void;
}

/** Every filter word in the option's text. */
function matches(o: PickerOption, words: string[]): boolean {
  const t = o.text.toLowerCase();
  return words.every((w) => t.includes(w));
}

export function Picker({ sections, value, placeholder, onPick, onClose, empty = 'Nothing matches.', className, foot, onStep }: PickerProps) {
  const [filter, setFilter] = useState('');
  const words = useMemo(() => filter.toLowerCase().split(/\s+/).filter(Boolean), [filter]);
  const shown = useMemo(() => sections.map((s) => ({ ...s, options: s.options.filter((o) => matches(o, words)) })).filter((s) => s.options.length > 0), [sections, words]);
  const pickable = useMemo(() => shown.flatMap((s) => s.options).filter((o) => !o.disabled), [shown]);
  const [active, setActive] = useState<string | null>(value);
  const box = useRef<HTMLDivElement>(null);
  const activeId = pickable.some((o) => o.id === active) ? active : (pickable[0]?.id ?? null);

  // A new filter starts from the best match.
  useEffect(() => {
    if (words.length > 0) setActive(pickable[0]?.id ?? null);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- keyed on the words typed: options that change under the same filter leave the highlight where you moved it
  }, [words.join(' ')]);

  useEffect(() => {
    box.current?.querySelector('.ca-picker-opt.is-on')?.scrollIntoView({ block: 'nearest' });
  }, [activeId]);

  // A press anywhere else closes it; the button that opened it closes it itself.
  useDismiss(box, onClose, { inside: '[data-picker-trigger]' });

  const move = (delta: number) => {
    if (pickable.length === 0) return;
    const i = pickable.findIndex((o) => o.id === activeId);
    setActive(pickable[Math.min(pickable.length - 1, Math.max(0, i + delta))].id);
  };

  return (
    <div ref={box} className={cx('ca-picker', className)} role="dialog">
      <input
        className="ca-picker-filter"
        value={filter}
        autoFocus
        spellCheck={false}
        autoCorrect="off"
        autoCapitalize="off"
        placeholder={placeholder}
        aria-label={placeholder}
        onChange={(e) => setFilter(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown' || (e.ctrlKey && e.key === 'n')) {
            e.preventDefault();
            move(1);
          } else if (e.key === 'ArrowUp' || (e.ctrlKey && e.key === 'p')) {
            e.preventDefault();
            move(-1);
          } else if (e.key === 'Enter') {
            e.preventDefault();
            e.stopPropagation();
            if (activeId) onPick(activeId);
          } else if (e.key === 'Escape') {
            e.preventDefault();
            e.stopPropagation();
            onClose();
          } else if (e.key === 'Tab') {
            onClose();
          } else if (onStep && !filter && (e.key === 'ArrowLeft' || e.key === 'ArrowRight') && !e.metaKey && !e.altKey && !e.shiftKey) {
            e.preventDefault();
            onStep(e.key === 'ArrowLeft' ? -1 : 1);
          }
        }}
      />
      <div className="ca-picker-list" role="listbox">
        {shown.length === 0 && <p className="ca-picker-empty">{empty}</p>}
        {shown.map((s) => (
          <div key={s.key} className="ca-picker-section" role="group">
            {s.head && <div className="ca-picker-head">{s.head}</div>}
            {s.options.map((o) => (
              <button
                key={o.id}
                type="button"
                role="option"
                aria-selected={o.id === value}
                disabled={o.disabled}
                title={o.title}
                className={cx('ca-picker-opt', o.id === activeId && 'is-on', o.id === value && 'is-chosen')}
                onMouseMove={() => !o.disabled && o.id !== activeId && setActive(o.id)}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => onPick(o.id)}
              >
                {o.lead}
                <span className="ca-picker-label">
                  <span className="ca-picker-title">{o.label}</span>
                  {o.detail && <span className="ca-picker-detail">{o.detail}</span>}
                </span>
                {o.trail}
              </button>
            ))}
          </div>
        ))}
      </div>
      {foot && <div className="ca-picker-foot">{foot}</div>}
    </div>
  );
}
