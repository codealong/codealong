// Rows picked together in a list (⌘-click, ⇧-click, ⇧↑ ⇧↓, ⌘A; picks.ts), shown in the panel in
// place of one row's: how many, what's done to all of them at once, and each of them, to show
// alone or leave out. Every list's picks go through this one panel, each filling it with its own
// actions and rows: sessions (SessionPickPanel.tsx), worktrees (WorktreePickPanel.tsx), PRs
// (PrPickPanel.tsx) and todos (TodoPanel.tsx `TodoPickPanel`).

import type { ReactNode } from 'react';
import { below } from '../contextMenu';
import { Button, cx, headerMenu, Icon } from './primitives';
import { keysIn } from '../platform';

interface PickPanelProps {
  /** What the panel is, for a screen reader: `Selected sessions`. */
  label: string;
  icon: string;
  /** How many are picked: `3 sessions`. */
  head: string;
  /** How picking works here, under the count. */
  help: string;
  /** The ⋯ button: everything that can be done to them, as their right-click menu has it. */
  moreTitle: string;
  /** That menu, dropped from the ⋯ button at `at`, or at the pointer for a right-click on the
   *  header. */
  onMoreMenu: (at?: { x: number; y: number }) => void;
  /** What's commonly done to all of them: a strip of buttons that say their keys (`PickKeys`), or
   *  a button of the list's own. */
  actions: ReactNode;
  /** The list of them (`PickList`), and anything the list says about it. */
  children: ReactNode;
}

export function PickPanel({ label, icon, head, help, moreTitle, onMoreMenu, actions, children }: PickPanelProps) {
  return (
    <aside className="ca-insp ca-pick" aria-label={label}>
      <header className="ca-pick-header" onContextMenu={headerMenu(() => onMoreMenu())}>
        <div className="ca-insp-title">
          <Icon name={icon} size={14} />
          <span className="ca-tag">Selected</span>
          <Button variant="ghost" size="sm" icon="dots" aria-label="More" title={moreTitle} className="ca-pick-more" onClick={(e) => onMoreMenu(below(e.currentTarget))} />
        </div>
        <h2 className="ca-pick-head">{head}</h2>
        <p className="ca-pick-help">{keysIn(help)}</p>
      </header>
      {actions}
      {children}
    </aside>
  );
}

/** The strip of what's done to all of them, the one a session's panel has over its box
 *  (SessionPanel.tsx `Hotkey`), at the panel's start. */
export function PickKeys({ children }: { children: ReactNode }) {
  return (
    <div className="ca-hotkeys ca-pick-keys" role="toolbar" aria-label="What’s commonly done to them">
      {children}
    </div>
  );
}

/** The ones picked, in the list's order; `numbered` where the order is what they're for (the
 *  prompt several todos start on). `label` heads it where the panel says more than the list. */
export function PickList({ label, numbered = false, children }: { label?: string; numbered?: boolean; children: ReactNode }) {
  const List = numbered ? 'ol' : 'ul';
  return (
    <section className="ca-insp-sec" aria-label={label ?? 'The ones selected'}>
      {label && <h4 className="ca-label">{label}</h4>}
      <List className="ca-pick-list">{children}</List>
    </section>
  );
}

interface PickRowProps {
  /** Its place in a numbered list. */
  n?: number;
  /** Its status, before its name. */
  lead: ReactNode;
  name: string;
  /** The name's tooltip, which says a click shows it alone. */
  title: string;
  onShow: () => void;
  onLeaveOut: () => void;
  /** Dimmed: archived, merged or closed. */
  dim?: boolean;
  /** What's said beside its name: its worktree, account, project. */
  children?: ReactNode;
}

/** One of them: its status and name, which a click shows alone, what's beside it, and a way to
 *  leave it out. */
export function PickRow({ n, lead, name, title, onShow, onLeaveOut, dim = false, children }: PickRowProps) {
  return (
    <li className={cx('ca-pick-row', dim && 'is-dim')}>
      {n != null && <span className="ca-pick-n">{n}</span>}
      {lead}
      <button type="button" className="ca-pick-name" title={title} onClick={onShow}>
        {name}
      </button>
      {children}
      <button type="button" className="ca-pick-out" aria-label={`Leave out ${name}`} title="Leave it out" onClick={onLeaveOut}>
        <Icon name="x" size={10} stroke={2.25} />
      </button>
    </li>
  );
}
