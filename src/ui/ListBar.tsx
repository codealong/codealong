// The strip over the main window's list, stuck to the top of the board as it scrolls: the view's
// tools (`ListTools`: Sessions' sort, SessionsView.tsx `SessionsBar`, and Rows in Sessions, PRs
// and Todo) and, in any view, the list's find while it's open. ⌘F from the list (findKeys.ts), and ⌥⌘F or / from anywhere in the window (mainKeys.ts),
// open the find, as ⌘F in a session's panel opens the find over its conversation (ThreadFind.tsx):
// what's typed narrows the view's list to what matches (boardLists.ts), and the canvas fades the
// cards that don't. The keys in the field are the window's (mainKeys.ts): ↓ hands the list the
// keyboard, ⏎ opens the selected row, Escape clears the find and puts it away. Left with nothing
// typed, it goes away by itself; left with words, it stays, narrowing the list, until they're
// cleared. Finding sessions, todos and PRs anywhere, and starting work, is the quick panel's, the
// box in the title bar.

import type { ReactNode, RefObject } from 'react';
import { Button, Icon, Seg } from './primitives';

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

/** Rows, among a view's tools: two lines a row, or one. */
export function RowsSeg({ options, value, onPick }: { options: RowsOption[]; value: RowLines; onPick: (lines: RowLines) => void }) {
  return <ListSeg label="Rows" options={options} value={value} onPick={onPick} />;
}

/** One group of a view's tools. */
export function ListSeg<T extends string>({ label, options, value, onPick }: { label: string; options: { id: T; label: string; icon: string; title: string }[]; value: T; onPick: (id: T) => void }) {
  return <Seg className="ca-listtools-seg" label={label} look="icon" iconSize={16} options={options} value={value} onPick={onPick} />;
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
