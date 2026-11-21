// The strip over the main window's list, sitting above the scrolling body: the view's
// tools (`ListTools`: its View menu, `ViewMenu`, with how it's sorted and grouped and how much a
// row says, and the filters narrowing it as chips, `FilterChips`), in any view the list's find
// while it's open, and at its end what the strip does (Sessions' Orchestrator). ⌥⌘F or / from anywhere in the
// window (mainKeys.ts) open it, as ⌥⌘F filters a PR's files where they show (findKeys.ts): ⌘F finds
// in the whole window without narrowing anything (WindowFind.tsx). What's typed in it narrows the
// view's list to what matches (boardLists.ts), and the canvas fades the
// cards that don't. The keys in the field are the window's (mainKeys.ts): ↓ hands the list the
// keyboard, ⏎ opens the selected row, Escape clears the find and puts it away. Left with nothing
// typed, it goes away by itself; left with words, it stays, narrowing the list, until they're
// cleared. Finding sessions, todos and PRs anywhere, and starting work, is the quick panel's, the
// box in the title bar.

import { useRef, useState, type ReactNode, type RefObject } from 'react';
import { Button, Chip, cx, Icon, Seg } from './primitives';
import { useDismiss } from './useDismiss';

export function ListBar({ children }: { children: ReactNode }) {
  return <div className="ca-listbar">{children}</div>;
}

/** A view's tools at the strip's start, outside the list's listbox: groups of icon buttons, each
 *  named by its tooltip. */
export function ListTools({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="ca-listtools" role="toolbar" aria-label={label}>
      {children}
    </div>
  );
}

/** How much each row of a list says: two lines, or one, the default. */
export type RowLines = 'two' | 'one';

export interface RowsOption {
  id: RowLines;
  label: string;
  icon: string;
  title: string;
}

/** Rows' two buttons, each tooltip saying what a row of the view shows that way. */
export function rowsOptions(two: string, one: string): RowsOption[] {
  return [
    { id: 'two', label: 'Two lines', icon: 'baseline-density-medium', title: `Two lines: ${two}` },
    { id: 'one', label: 'One line', icon: 'baseline-density-small', title: `One line: ${one}` },
  ];
}

/** One choice of a view's settings: how it sorts, groups or how much a row says. */
export interface ListOption<T extends string> {
  id: T;
  label: string;
  icon: string;
  title: string;
}

/** One of a view's settings in its View menu: its name, its choices as a segmented control, and
 *  what the View button says of it while it's off its default (`viewSetting`). */
export interface ViewSetting {
  label: string;
  control: ReactNode;
  /** The words the button adds for it, "by repo"; null at its default, where it says nothing. */
  said: string | null;
}

/** A setting for the View menu. `plain` is its default, which the button leaves unsaid; `say`
 *  words the choice that isn't, from its label. */
export function viewSetting<T extends string>(label: string, options: ListOption<T>[], value: T, plain: T, onPick: (id: T) => void, say: (label: string) => string): ViewSetting {
  const on = options.find((o) => o.id === value);
  return {
    label,
    said: value !== plain && on ? say(on.label.toLowerCase()) : null,
    control: <Seg className="ca-viewmenu-seg" label={label} look="text" options={options} value={value} onPick={onPick} />,
  };
}

/** The words for a setting that's a sort or a grouping: "by repo". */
export const byWords = (label: string) => `by ${label}`;

/** Rows' setting: two lines a row, or one, the default. */
export function rowsSetting(options: RowsOption[], value: RowLines, onPick: (lines: RowLines) => void): ViewSetting {
  return viewSetting('Rows', options, value, 'one', onPick, (label) => label);
}

/** A view's settings behind one button at the strip's start: how its list is sorted and grouped
 *  and how much a row says. The button names whatever isn't at its default, so the list's look
 *  reads from the strip without opening it; the popover stays up while you change several, and a
 *  press outside it or Escape puts it away. */
export function ViewMenu({ label, settings }: { label: string; settings: ViewSetting[] }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useDismiss(ref, () => setOpen(false), { when: open });
  const said = settings.flatMap((s) => (s.said ? [s.said] : []));
  return (
    <div className="ca-viewmenu" ref={ref}>
      <button
        type="button"
        className={cx('ca-viewmenu-btn', said.length > 0 && 'is-changed')}
        aria-haspopup="dialog"
        aria-expanded={open}
        title={said.length ? `${label}: ${said.join(', ')}` : `${label}: sort, groups and rows`}
        onClick={() => setOpen((o) => !o)}
      >
        <Icon name="adjustments-horizontal" size={15} />
        <span>View</span>
        {said.length > 0 && <span className="ca-viewmenu-said">{said.join(' · ')}</span>}
        <Icon name="chevron-down" size={12} />
      </button>
      {open && (
        <div className="ca-menu ca-viewmenu-pop" role="dialog" aria-label={label}>
          {settings.map((s) => (
            <div key={s.label} className="ca-viewmenu-row">
              <span className="ca-viewmenu-label">{s.label}</span>
              {s.control}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/** A filter narrowing the list, as a chip on the strip: its name, and how to take it off. */
export interface ListFilter {
  key: string;
  label: string;
  /** What it narrows to, which its × names: "Repos: atlas/api". */
  name: string;
  /** Its tooltip, where it says more than `name`: a label's description. */
  title?: string;
  clear: () => void;
}

/** The filters that narrow the view's list now, the sidebar's, as chips after the View button:
 *  each one's × takes it off, and with several, Clear takes them all. Nothing shows while none
 *  is on. */
export function FilterChips({ filters, onClearAll }: { filters: ListFilter[]; onClearAll: () => void }) {
  if (filters.length === 0) return null;
  return (
    <div className="ca-listfilters" role="group" aria-label="Filters">
      {filters.map((f) => (
        <Chip key={f.key} className="ca-listfilter" title={`Filtered to ${f.title ?? f.name}`} remove={{ label: `Stop filtering to ${f.name}`, title: 'Stop filtering', run: f.clear }}>
          <span className="ca-listfilter-label">{f.label}</span>
        </Chip>
      ))}
      {filters.length > 1 && (
        <button type="button" className="ca-linkbtn ca-listfilters-clear" onClick={onClearAll}>
          Clear
        </button>
      )}
    </div>
  );
}

interface ListFindProps {
  inputRef: RefObject<HTMLInputElement | null>;
  value: string;
  onChange: (value: string) => void;
  /** What the view finds in: "Find in sessions". */
  placeholder: string;
  /** The keyboard left the field. */
  onLeave: () => void;
  /** Clears what's typed and puts the find away. */
  onClose: () => void;
}

export function ListFind({ inputRef, value, onChange, placeholder, onLeave, onClose }: ListFindProps) {
  return (
    <div className="ca-listfind" role="search">
      <label className="ca-listfind-field">
        <Icon name="search" size={12} />
        <input ref={inputRef} value={value} placeholder={placeholder} aria-label={placeholder} spellCheck={false} onChange={(e) => onChange(e.target.value)} onBlur={onLeave} />
      </label>
      {/* The cursor stays in the field until the find goes away. */}
      <Button variant="ghost" size="sm" icon="x" title="Close (esc)" aria-label="Close find" onMouseDown={(e) => e.preventDefault()} onClick={onClose} />
    </div>
  );
}
