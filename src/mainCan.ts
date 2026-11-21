// What the main window can do to a session and to a worktree (menus.ts `SessionCan`, `LaneCan`),
// for their right-click menus and for its keys, which run the same actions. App.tsx says what the
// window has on each render (`MainHas`), and a session's or a worktree's is made from that as
// its menu opens or a key asks: with what only this window does (rename it in a sheet, add it to
// a todo, pin it, add it to a board, undo an archive, follow a session it moves to its copy).

import type { Dispatch, SetStateAction } from 'react';
import type { HelperCount, ListRow, RunCount, SideCount } from './boardLists';
import type { RefKind } from './canvas/model';
import type { MenuSpec } from './contextMenu';
import type { View } from './mainViews';
import * as menus from './menus';
import type { Pins } from './pins';
import { errorText, type Report } from './report';
import { relatedMenuItems } from './rowSignal';
import { ownTerminalFor } from './terminals';
import type { Board, Label, Lane, Session } from './types';
import { runFolder } from './ui/RunButton';
import type { SessionItem } from './ui/SessionsView';
import type { SettingsSection } from './ui/SettingsDialog';
import type { Archived } from './ui/Toasts';
import type { ComposerContext } from './ui/TodoView';

/** What the window has, as one render has it. */
export interface MainHas {
  report: Report;
  view: View;
  /** Opens one of a worktree's targets where it lives. */
  open: (lane: Lane, target: string) => void;
  board: Board | null;
  editors: menus.Editors | undefined;
  newSession: (at: { laneId: string | null }) => void;
  /** Add to todo and Remove from todo (useTodoView.ts `todoMenuItems`), as the latest render has them. */
  todoItems: { readonly current: (project: string, link: { worktree?: string; session?: string }, context: ComposerContext, title?: string) => MenuSpec };
  sessionPins: Pins;
  lanePins: Pins;
  toggleLanePin: (lane: Lane) => void;
  /** What's pinned to the desktop, and the sessions kept in windows of their own. */
  onDesktop: ReadonlySet<string>;
  keptSessions: ReadonlySet<string>;
  /** The sheet that renames a session, by its key. */
  setRenaming: (key: string) => void;
  /** It left what waits on you: on to the next from the one shown in Sessions. */
  leftAndOn: (s: Session, done: menus.Left) => void;
  /** Archives sessions or takes them out of the archive, with the toast's Undo; and the Undo alone. */
  setArchived: (keys: string[], archived: boolean, title: string) => void;
  setArchivedUndo: (undo: Archived | null) => void;
  /** Sessions' queue, for one whose merge was refused to come round again. */
  queue: { again: (key: string) => void };
  /** Merge and done…'s dialog, and Mark as done's question about helpers. */
  mergingDone: { ask: (ask: menus.MergeDoneAsk) => boolean };
  doneHelpers: menus.DoneHelpersCan | undefined;
  /** The row selected, and the session shown in Sessions as you moved it. */
  selected: ListRow | null;
  setMoving: Dispatch<SetStateAction<{ from: SessionItem; to: string | null } | null>>;
  /** Add to board › for one thing. */
  boardItems: (ref: RefKind, keys: string[]) => MenuSpec;
  /** Counts and toggle state for finished helpers, headless runs and side conversations in Sessions. */
  helpersOf?: (sessionKey: string) => HelperCount | undefined;
  runsOf?: (sessionKey: string) => RunCount | undefined;
  sidesOf?: (sessionKey: string) => SideCount | undefined;
  helpersShown?: ReadonlySet<string>;
  toggleHelpers?: (shownKey: string) => void;
  todoStore: { labels: Label[] };
  /** The worktree whose preview URL is being set, Settings at a section, and Clean up worktree…. */
  setPreviewFor: (id: string) => void;
  openSettings: (section?: SettingsSection, focus?: string | null) => void;
  setCleaning: (lanes: Lane[]) => void;
}

/**
 * What this window can do to a session, for its menu and its keys. Each list pins what it shows:
 * Sessions pins the session; the board and the sidebar, lists of worktrees, pin the session's
 * worktree.
 */
export function sessionCanOf(has: MainHas, lane: Lane, s: Session, pin: 'session' | 'worktree'): menus.SessionCan {
  const { report, view, board, sessionPins, lanePins, selected } = has;
  return {
    report,
    keys: view === 'sessions' ? 'chords' : true,
    open: has.open,
    terminal: board?.apps.terminal ?? 'Terminal',
    editors: has.editors,
    newSession: () => has.newSession({ laneId: lane.id }),
    ownTerminal: ownTerminalFor(s.key, runFolder(lane, s), board?.apps.runIn, board?.terminals?.[s.key], (e: unknown) => report.error(errorText(e))),
    todos: has.todoItems.current(lane.project, { session: s.key }, { session: s.key, worktree: lane.id, label: `${s.account} in ${lane.callsign}` }, s.title),
    pin:
      pin === 'session'
        ? { label: sessionPins.set.has(s.key) ? 'Unpin session' : 'Pin session', run: () => sessionPins.toggle(s.key) }
        : { label: lanePins.set.has(lane.id) ? 'Unpin worktree' : 'Pin worktree', run: () => has.toggleLanePin(lane) },
    desktop: has.onDesktop,
    windows: has.keptSessions,
    rename: () => has.setRenaming(s.key),
    worktrees: board?.lanes,
    archive: (archived) => {
      if (archived) has.leftAndOn(s, 'archived');
      has.setArchived([s.key], archived, s.title);
    },
    // Undo takes it out of the archive; a terminal it closed stays closed, and it resumes as any other.
    undoDone: (message) => has.setArchivedUndo({ keys: [s.key], title: s.title, message }),
    left: (done) => has.leftAndOn(s, done),
    // Its merge refused: round again in Sessions' queue, with a toast where the dialog didn't
    // come back (another was up).
    failed: (why) => {
      has.queue.again(s.key);
      if (why) report.error(why);
    },
    mergeDone: board ? { prs: board.prs, ask: has.mergingDone.ask } : undefined,
    doneHelpers: has.doneHelpers,
    // Moved from its panel in Sessions, it stays there until its copy takes its place, rather
    // than the panel going on to the next that waits.
    moving: (copy) => {
      if (view !== 'sessions' || selected?.kind !== 'agent' || selected.session.key !== s.key) return;
      const from = selected;
      has.setMoving({ from, to: null });
      void copy.then((to) => has.setMoving((m) => (m?.from !== from ? m : to ? { from, to } : null)));
    },
    boards: has.boardItems('session', [s.key]),
    related:
      view === 'sessions' && has.toggleHelpers && has.helpersShown
        ? relatedMenuItems(
            s.key,
            { helpers: has.helpersOf?.(s.key), runs: has.runsOf?.(s.key), sides: has.sidesOf?.(s.key) },
            has.helpersShown,
            has.toggleHelpers,
          )
        : [],
    labels: has.todoStore.labels,
  } satisfies menus.CanIn<'main'>;
}

/** What this window can do to a worktree, for its menu and its keys. */
export function laneCanOf(has: MainHas, lane: Lane): menus.LaneCan {
  const { lanePins } = has;
  return {
    report: has.report,
    keys: has.view === 'sessions' ? 'chords' : true,
    open: has.open,
    editors: has.editors,
    newSession: () => has.newSession({ laneId: lane.id }),
    setPreview: () => has.setPreviewFor(lane.id),
    automation: (repo) => has.openSettings('automation', repo),
    todos: has.todoItems.current(lane.project, { worktree: lane.id }, { worktree: lane.id, label: lane.callsign }),
    pin: { label: lanePins.set.has(lane.id) ? 'Unpin worktree' : 'Pin worktree', run: () => has.toggleLanePin(lane) },
    desktop: has.onDesktop,
    boards: has.boardItems('worktree', [lane.id]),
    cleanUp: has.setCleaning,
  };
}
