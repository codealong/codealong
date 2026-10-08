// Rows picked together in the main window's lists (⌘-click, ⇧-click, ⇧↑ ⇧↓, ⌘A; picks.ts):
// sessions in Sessions, lines on the Worktrees board and PRs in PRs, each with what's done to all
// of them (menus.ts `sessionsActions`, `boardActions`, `prsActions`) for their right-click menu,
// their panel (PickPanel.tsx) and the keys (mainKeys.ts `ListPicks`). The Todo view's picks, for
// starting one session on several todos, are its own (useTodoView.ts).

import { useMemo } from 'react';
import type { ListRow } from './boardLists';
import { showContextMenu, type MenuAction, type MenuSpec } from './contextMenu';
import { count } from './format';
import type { BoardRow } from './lanes';
import * as menus from './menus';
import { usePicks, type SelectHow } from './picks';
import type { Pins } from './pins';
import type { Report } from './report';
import type { PrItem } from './types';
import type { PrRowItem } from './ui/PrsView';
import type { SessionItem } from './ui/SessionsView';
import type { Archived } from './ui/Toasts';

/** What the picks go by, as the window has it. */
export interface ListPicksInput {
  view: string;
  /** Nothing is listed until the board is read, so nothing is picked. */
  loading: boolean;
  /** The key of the row selected, as the window resolves it. */
  selected: string | null;
  setSelectedKey: (key: string) => void;
  sessionRows: ListRow[];
  worktreeItems: BoardRow[];
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
}

/** One list's picks: the rows, in the list's order, and what's done to them. */
export interface ListPick<R, A> {
  rows: R[];
  keys: ReadonlySet<string>;
  select: (row: R, how: SelectHow) => void;
  actions: () => A;
  /** Their right-click menu, and the panel's ⋯ menu at `at`. */
  menu: (at?: { x: number; y: number }) => void;
}

export function useListPicks(input: ListPicksInput) {
  const { view, loading, selected, setSelectedKey, sessionRows, worktreeItems, prVisible, showArchived, report, setToast, sessionPins, lanePins, prPins, archive, undoArchive, approve } = input;
  const sessionItems = useMemo(() => (view === 'sessions' && !loading ? sessionRows.filter((r): r is SessionItem => r.kind === 'agent') : null), [view, loading, sessionRows]);
  const boardItems = useMemo(() => (view === 'worktrees' && !loading ? worktreeItems : null), [view, loading, worktreeItems]);
  const prItems = useMemo(() => (view === 'prs' && !loading ? prVisible.filter((r): r is PrRowItem => r.kind === 'pr') : null), [view, loading, prVisible]);
  const sessions = usePicks(sessionItems, selected, setSelectedKey);
  const board = usePicks(boardItems, selected, setSelectedKey);
  const prs = usePicks(prItems, selected, setSelectedKey);
  const showMenu = (spec: MenuSpec, at?: { x: number; y: number }) => void showContextMenu(spec, at).catch((e: unknown) => setToast(String(e)));

  /** Pins or unpins `keys` in `pins`: pins the ones that aren't, else unpins them all. */
  const pinItem = (pins: Pins, keys: string[], word: string): MenuAction | undefined => {
    if (keys.length === 0) return undefined;
    const unpinned = keys.filter((k) => !pins.set.has(k));
    return unpinned.length > 0 ? { label: `Pin ${count(unpinned.length, word)}`, run: () => pins.pinAll(unpinned, true) } : { label: `Unpin ${count(keys.length, word)}`, run: () => pins.pinAll(keys, false) };
  };

  /** What's done to sessions, here or on the board: ones that leave the list (done, archived)
   *  leave the pick, and with none left the row after them is selected; the rest stay picked, to
   *  do more to (a working one Mark as done held, say). */
  const sessionsCan = (rows: { key: string; session: { key: string; title: string; archived?: boolean } }[], leave: (gone: ReadonlySet<string>) => void, stays: boolean): menus.SessionsCan => ({
    report,
    keys: 'chords',
    archive: (keys, archived) => archive(keys, archived, rows.find((r) => r.session.key === keys[0])?.session.title ?? ''),
    // Undo takes them out of the archive, as it does one.
    undoDone: (keys, message) => undoArchive({ keys, title: '', message }),
    left: (done, keys) => {
      if ((done !== 'done' && done !== 'archived') || stays) return;
      leave(new Set(rows.filter((r) => keys.includes(r.session.key)).map((r) => r.key)));
    },
  });

  // Sessions pins the sessions; archived ones stay listed only in the Archived fold, shown.
  const sessionsOf = (rows: SessionItem[]): menus.SessionsCan => ({
    ...sessionsCan(rows, sessions.leave, showArchived),
    pin: pinItem(sessionPins, rows.filter((r) => !r.session.archived).map((r) => r.session.key), 'session'),
  });
  // The board pins worktrees, as P does there; done and archived sessions leave it.
  const boardOf = (rows: BoardRow[]): menus.SessionsCan => ({
    ...sessionsCan(rows.flatMap((r) => (r.kind === 'agent' ? [r] : [])), board.leave, false),
    keys: true,
    pin: pinItem(lanePins, [...new Set(rows.map((r) => r.lane.id))], 'worktree'),
  });
  const prsOf = (): menus.PrsCan => ({ report, keys: true, approve, pin: pinItem(prPins, prs.picked.map((r) => r.pr.key), 'PR') });
  const pickedPrs = prs.picked.map((r) => r.pr);

  const sessionPick: ListPick<SessionItem, menus.SessionsActions> = {
    rows: sessions.picked,
    keys: sessions.keys,
    select: sessions.select,
    actions: () => menus.sessionsActions(sessions.picked, sessionsOf(sessions.picked)),
    menu: (at) => showMenu(menus.sessionsMenu(sessions.picked, sessionsOf(sessions.picked)), at),
  };
  const boardPick: ListPick<BoardRow, menus.BoardActions> = {
    rows: board.picked,
    keys: board.keys,
    select: board.select,
    actions: () => menus.boardActions(board.picked, boardOf(board.picked)),
    menu: (at) => showMenu(menus.boardMenu(board.picked, boardOf(board.picked)), at),
  };
  const prPick: ListPick<PrRowItem, menus.PrsActions> = {
    rows: prs.picked,
    keys: prs.keys,
    select: prs.select,
    actions: () => menus.prsActions(pickedPrs, prsOf()),
    menu: (at) => showMenu(menus.prsMenu(pickedPrs, prsOf()), at),
  };

  // The view's picks, as the keys go by them.
  const live = view === 'sessions' ? { p: sessions, pick: sessionPick } : view === 'worktrees' ? { p: board, pick: boardPick } : view === 'prs' ? { p: prs, pick: prPick } : null;
  const keys: ListPicksKeys | null = live
    ? {
        count: live.pick.rows.length,
        range: (row) => {
          if (view === 'sessions' && row.kind === 'agent') sessions.select(row, 'range');
          else if (view === 'worktrees' && (row.kind === 'agent' || row.kind === 'lane')) board.select(row, 'range');
          else if (view === 'prs' && row.kind === 'pr') prs.select(row, 'range');
        },
        all: live.p.all,
        clear: live.p.clear,
        actions: live.pick.actions,
      }
    : null;
  return { sessions: sessionPick, board: boardPick, prs: prPick, keys };
}

/** The view's picks as the keys go by them (mainKeys.ts): how many, ⇧↑ ⇧↓'s run, ⌘A, Esc, and
 *  what the keys run on all of them, by the names one row's actions have. */
export interface ListPicksKeys {
  count: number;
  range: (row: ListRow) => void;
  all: () => void;
  clear: () => void;
  actions: () => menus.SessionsActions | menus.BoardActions | menus.PrsActions | null;
}
