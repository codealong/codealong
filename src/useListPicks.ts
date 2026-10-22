// Rows picked together in the main window's lists (⌘-click, ⇧-click, ⇧↑ ⇧↓, ⌘A; picks.ts):
// sessions in Sessions, worktrees by their headings in Sessions grouped by worktree, and PRs in
// PRs, each with what's done to all of them (menus.ts `sessionsActions`,
// `lanesActions`, `prsActions`) for their right-click menu, their panel (PickPanel.tsx) and the
// keys (mainKeys.ts). The Todo view's picks, for starting one session on several todos, are its
// own (useTodoView.ts).

import { useMemo } from 'react';
import type { ListRow } from './boardLists';
import { showContextMenu, type MenuAction, type MenuSpec } from './contextMenu';
import { count } from './format';
import type { RefKind } from './canvas/model';
import type { LaneItem } from './lanes';
import * as menus from './menus';
import { usePicks, type SelectHow } from './picks';
import type { Pins } from './pins';
import type { Report } from './report';
import type { Lane, PrItem } from './types';
import type { PrRowItem } from './ui/PrsView';
import type { SessionItem } from './ui/SessionsView';
import type { Archived } from './ui/Toasts';

/** What the picks go by, as the window has it. */
export interface ListPicksInput {
  view: string;
  /** Nothing is listed until the board is read, so nothing is picked. */
  loading: boolean;
  /** The row selected, as the window resolves it. */
  selected: ListRow | null;
  setSelectedKey: (key: string) => void;
  /** Sessions' rows, top to bottom: sessions, and grouped by worktree, the worktrees' headings. */
  sessionRows: ListRow[];
  prVisible: ListRow[];
  /** Sessions' Archived fold is open, so archived sessions stay listed. */
  showArchived: boolean;
  report: Report;
  setToast: (message: string | null) => void;
  sessionPins: Pins;
  lanePins: Pins;
  prPins: Pins;
  /** Archives sessions or takes them out of the archive, with the toast's Undo. */
  archive: (keys: string[], archived: boolean, title: string) => void;
  /** Offers the toast's Undo for sessions Mark as done archived. */
  undoArchive: (undo: Archived) => void;
  /** Approves PRs as you, one after another. */
  approve: (prs: PrItem[]) => void;
  /** Asks to remove worktrees that can go (ui/CleanUpDialog.tsx). */
  cleanUp: (lanes: Lane[]) => void;
  /** Add to board › for these, as one's menu has it. */
  boardItems: (ref: RefKind, keys: string[]) => MenuSpec;
}

/** One list's picks: the rows, in the list's order, and what's done to them. */
export interface ListPick<R, A> {
  rows: R[];
  keys: ReadonlySet<string>;
  select: (row: R, how: SelectHow) => void;
  /** Goes back to one, leaving the selection where it is. */
  clear: () => void;
  actions: () => A;
  /** Their right-click menu, and the panel's ⋯ menu at `at`. */
  menu: (at?: { x: number; y: number }) => void;
}

/** The view's picks as the keys go by them (mainKeys.ts): how many, ⇧↑ ⇧↓, ⌘A, Esc, and what
 *  the keys run on all of them, by the names one row's actions have. */
export interface ListPicksKeys {
  count: number;
  /** Picks the row above or below too, as ⇧-click does. */
  step: (dir: 1 | -1) => void;
  all: () => void;
  clear: () => void;
  actions: () => menus.SessionsActions | menus.LanesActions | menus.PrsActions | null;
}

export function useListPicks(input: ListPicksInput) {
  const { view, loading, selected, setSelectedKey, sessionRows, prVisible, showArchived, report, setToast, sessionPins, lanePins, prPins, archive, undoArchive, approve, cleanUp, boardItems } = input;
  const sessionItems = useMemo(() => (view === 'sessions' && !loading ? sessionRows.filter((r): r is SessionItem => r.kind === 'agent') : null), [view, loading, sessionRows]);
  const laneItems = useMemo(() => (view === 'sessions' && !loading ? sessionRows.filter((r): r is LaneItem => r.kind === 'lane') : null), [view, loading, sessionRows]);
  const prItems = useMemo(() => (view === 'prs' && !loading ? prVisible.filter((r): r is PrRowItem => r.kind === 'pr') : null), [view, loading, prVisible]);
  // Sessions and worktrees aren't picked together: a ⌘-click on a heading with a session selected
  // starts a pick of worktrees.
  const selectedLane = selected?.kind === 'lane' ? selected.key : null;
  const sessions = usePicks(sessionItems, selected?.key ?? null, setSelectedKey);
  const worktrees = usePicks(laneItems, selectedLane, setSelectedKey);
  const prs = usePicks(prItems, selected?.key ?? null, setSelectedKey);
  const showMenu = (spec: MenuSpec, at?: { x: number; y: number }) => void showContextMenu(spec, at).catch((e: unknown) => setToast(String(e)));

  /** Pins or unpins `keys` in `pins`: pins the ones that aren't, else unpins them all. */
  const pinItem = (pins: Pins, keys: string[], word: string): MenuAction | undefined => {
    if (keys.length === 0) return undefined;
    const unpinned = keys.filter((k) => !pins.set.has(k));
    return unpinned.length > 0 ? { label: `Pin ${count(unpinned.length, word)}`, run: () => pins.pinAll(unpinned, true) } : { label: `Unpin ${count(keys.length, word)}`, run: () => pins.pinAll(keys, false) };
  };

  /** What's done to sessions: ones that leave the list (done, or archived while the Archived fold
   *  is shut) leave the pick, and with none left the row after them is selected; the rest stay
   *  picked, to do more to (a working one Mark as done held, say). */
  const sessionsOf = (rows: SessionItem[]): menus.SessionsCan => ({
    report,
    keys: 'chords',
    pin: pinItem(sessionPins, rows.filter((r) => !r.session.archived).map((r) => r.session.key), 'session'),
    // Not one shown in a worktree it doesn't run in (a visitor), which one's menu leaves out too.
    boards: boardItems('session', rows.filter((r) => !r.session.visitor).map((r) => r.session.key)),
    archive: (keys, archived) => archive(keys, archived, rows.find((r) => r.session.key === keys[0])?.session.title ?? ''),
    // Undo takes them out of the archive, as it does one.
    undoDone: (keys, message) => undoArchive({ keys, title: '', message }),
    left: (done, keys) => {
      if ((done !== 'done' && done !== 'archived') || showArchived) return;
      sessions.leave(new Set(rows.filter((r) => keys.includes(r.session.key)).map((r) => r.key)));
    },
  });
  const lanesOf = (rows: LaneItem[]): menus.LanesCan => ({ report, keys: 'chords', pin: pinItem(lanePins, rows.map((r) => r.lane.id), 'worktree'), cleanUp, boards: boardItems('worktree', rows.map((r) => r.lane.id)) });
  const prsOf = (rows: PrRowItem[]): menus.PrsCan => ({ report, keys: true, approve, pin: pinItem(prPins, rows.map((r) => r.pr.key), 'PR'), boards: boardItems('pr', rows.map((r) => r.pr.key)) });
  const lanes = worktrees.picked.map((r) => r.lane);
  const pickedPrs = prs.picked.map((r) => r.pr);

  const sessionPick: ListPick<SessionItem, menus.SessionsActions> = {
    rows: sessions.picked,
    keys: sessions.keys,
    select: sessions.select,
    clear: sessions.clear,
    actions: () => menus.sessionsActions(sessions.picked, sessionsOf(sessions.picked)),
    menu: (at) => showMenu(menus.sessionsMenu(sessions.picked, sessionsOf(sessions.picked)), at),
  };
  const worktreePick: ListPick<LaneItem, menus.LanesActions> = {
    rows: worktrees.picked,
    keys: worktrees.keys,
    select: worktrees.select,
    clear: worktrees.clear,
    actions: () => menus.lanesActions(lanes, lanesOf(worktrees.picked)),
    menu: (at) => showMenu(menus.lanesMenu(lanes, lanesOf(worktrees.picked)), at),
  };
  const prPick: ListPick<PrRowItem, menus.PrsActions> = {
    rows: prs.picked,
    keys: prs.keys,
    select: prs.select,
    clear: prs.clear,
    actions: () => menus.prsActions(pickedPrs, prsOf(prs.picked)),
    menu: (at) => showMenu(menus.prsMenu(pickedPrs, prsOf(prs.picked)), at),
  };

  /** ⇧↑ ⇧↓ in a list of `rows`, from the one selected, `at`. */
  const stepIn = <R extends { key: string }>(rows: R[] | null, at: string | null, select: (row: R, how: SelectHow) => void, dir: 1 | -1) => {
    if (!rows || at === null) return;
    const i = rows.findIndex((r) => r.key === at);
    const to = rows[Math.min(rows.length - 1, Math.max(0, i + dir))];
    if (to && i >= 0) select(to, 'range');
  };
  // The view's picks, as the keys go by them.
  // In Sessions, the worktrees' while a heading is selected or they're picked, else the sessions'.
  const keys: ListPicksKeys | null =
    view === 'sessions' && (worktrees.picked.length > 0 || selectedLane !== null)
      ? { count: worktrees.picked.length, step: (dir) => stepIn(laneItems, selectedLane, worktrees.select, dir), all: worktrees.all, clear: worktrees.clear, actions: worktreePick.actions }
      : view === 'sessions'
        ? { count: sessions.picked.length, step: (dir) => stepIn(sessionItems, selected?.key ?? null, sessions.select, dir), all: sessions.all, clear: sessions.clear, actions: sessionPick.actions }
        : view === 'prs'
          ? { count: prs.picked.length, step: (dir) => stepIn(prItems, selected?.key ?? null, prs.select, dir), all: prs.all, clear: prs.clear, actions: prPick.actions }
          : null;
  return { sessions: sessionPick, worktrees: worktreePick, prs: prPick, keys };
}
