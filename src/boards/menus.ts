// The menus of a board, of a column of its Kanban, and Add to board › in the menus of what a
// board holds: what each lists, as pure functions of what the window has and what a pick asks of
// it (the main window's are wired in useBoardEdits.ts). Apart from menus.ts, which has the menus
// of sessions, worktrees, PRs and todos: these are a board's own, and that file is long.

import type { RefKind } from '../canvas/model';
import type { MenuSpec } from '../contextMenu';
import { count } from '../format';
import { boardChoices, boardName, ONE_WORD, type BoardColumn, type BoardDoc } from './model';

export interface BoardCan {
  /** The project's boards, to show one of. */
  boards: readonly BoardDoc[];
  /** The board the Board view shows now, by id; none while another view shows. */
  showing: string | null;
  /** The project the window shows: a board of another says its own (`boardName`). */
  current: string;
  /** The projects a board can move to. */
  projects: readonly string[];
  show: (id: string) => void;
  /** New board…, and Settings of a board…: the dialog that makes or changes one. */
  create: () => void;
  settings: (b: BoardDoc) => void;
  move: (b: BoardDoc, project: string) => void;
  /** Its Kanban's columns back to the stages. */
  stages: (b: BoardDoc) => void;
  remove: (b: BoardDoc) => void;
}

/** A board's menu, or with none the Board view's: the project's boards to show, a new one, and
 *  the board's settings, moving it to another project as a todo moves, deleting it. */
export function boardMenu(b: BoardDoc | null, can: BoardCan): MenuSpec {
  return [
    ...can.boards.map((x) => ({ label: boardName(x, can.current), checked: x.id === can.showing, run: () => can.show(x.id) })),
    ...(can.boards.length > 0 ? ['separator' as const] : []),
    { label: 'New board…', run: can.create },
    ...(b
      ? [
          { label: `Settings of ${b.name}…`, run: () => can.settings(b) },
          { label: 'Move to project', items: can.projects.map((p) => ({ label: p, checked: b.project === p, run: () => can.move(b, p) })) },
          ...(b.columns ? [{ label: 'Kanban columns back to the stages', run: () => can.stages(b) }] : []),
          'separator' as const,
          { label: `Delete ${b.name}`, run: () => can.remove(b) },
        ]
      : []),
  ];
}

export interface ColumnCan {
  /** Column settings…, and Add column…: the dialog that changes or makes one. */
  settings: (column: BoardColumn) => void;
  add: () => void;
  /** The board's columns as a pick leaves them. */
  set: (columns: BoardColumn[]) => void;
}

/** A Kanban column's menu (KanbanView.tsx), among the board's columns `cols`: its settings,
 *  folding it, moving it, taking it away. None for a column the board doesn't list, or one that
 *  isn't the board's own. */
export function columnMenu(cols: BoardColumn[], c: { id: string; column: BoardColumn | null }, can: ColumnCan): MenuSpec | null {
  const at = cols.findIndex((x) => x.id === c.id);
  if (at < 0 || !c.column) return null;
  const col = c.column;
  const swap = (i: number, j: number) => can.set(cols.map((x, k) => (k === i ? cols[j] : k === j ? cols[i] : x)));
  return [
    { label: 'Column settings…', run: () => can.settings(col) },
    { label: col.folded ? 'Open at first' : 'Fold at first', run: () => can.set(cols.map((x) => (x.id === col.id ? { ...x, folded: !col.folded } : x))) },
    'separator',
    { label: 'Move left', enabled: at > 0, run: () => swap(at, at - 1) },
    { label: 'Move right', enabled: at < cols.length - 1, run: () => swap(at, at + 1) },
    'separator',
    { label: 'Add column…', run: can.add },
    { label: `Delete ${col.name}`, enabled: cols.length > 1, run: () => can.set(cols.filter((x) => x.id !== col.id)) },
  ];
}

export interface AddToBoardCan {
  /** The project's boards, and the project the window shows. */
  boards: BoardDoc[];
  current: string;
  /** Whether a board holds a card, asked of each board once (model.ts `boardChoices`). */
  holds: (b: BoardDoc) => (card: string) => boolean;
  /** The board whose Kanban lists them, and whether it holds a card: Remove from board is for it.
   *  None where another view lists them. */
  kanban: { board: BoardDoc; member: (card: string) => boolean } | null;
  /** Puts cards on a board, or takes them off. */
  put: (b: BoardDoc, on: boolean, cards: string[]) => void;
  /** New board with it…: the dialog, for a board that starts with these on it. */
  create: (cards: string[]) => void;
}

/** Add to board › for one thing or several of a kind picked together, each with its card and
 *  its project: each of the boards that would show them checked when it holds them (a pick puts
 *  them on or takes them off, `boardChoices`), saying how many of them it takes where that's
 *  fewer than all; and in Kanban, Remove from board for the board shown. */
export function addToBoard(ref: RefKind, cards: { card: string; project: string | null }[], can: AddToBoardCan): MenuSpec {
  const several = cards.length > 1;
  const many = (n: number) => (several ? ` ${count(n, ONE_WORD[ref])}` : '');
  const choices = boardChoices(can.boards, cards, can.holds).map(({ board: b, cards: list, fresh, on }) => ({
    label: list.length < cards.length ? `${boardName(b, can.current)} (${list.length} of ${cards.length})` : boardName(b, can.current),
    checked: on,
    run: () => can.put(b, !on, on ? list : fresh),
  }));
  const shown = can.kanban;
  const held = shown ? cards.map((c) => c.card).filter(shown.member) : [];
  const remove: MenuSpec = shown && held.length > 0 ? [{ label: `Remove${many(held.length)} from board`, run: () => can.put(shown.board, false, held) }] : [];
  return [
    {
      label: `Add${many(cards.length)} to board`,
      items: [...choices, 'separator', { label: several ? 'New board with them…' : 'New board with it…', run: () => can.create(cards.map((c) => c.card)) }],
    },
    ...remove,
  ];
}
