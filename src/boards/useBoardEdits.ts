// Making and changing the boards from the main window (App.tsx): the board or the column its
// dialog makes or changes (ui/BoardDialog.tsx), and what its menus and the dialogs' Save do with
// it: showing a board, making one, renaming it, moving it to another project (the window follows
// the board it shows there), its Kanban's columns, and Add to board › in every other menu. The
// menus themselves are boards/menus.ts's. Each change goes to the engine through the window's
// boards (useBoards.ts), which shows it at once.

import { useMemo, useState } from 'react';
import { projectOfCard } from '../boardLists';
import { refId, type RefKind } from '../canvas/model';
import { showContextMenu, type MenuSpec } from '../contextMenu';
import { useLatest } from '../latest';
import { ALL_PROJECTS } from '../quick';
import { errorText } from '../report';
import { todoProjects } from '../todos';
import type { Board, Label, Lane, Todo } from '../types';
import type { BoardLayout } from '../ui/BoardBar';
import type { BoardDraft } from '../ui/BoardDialog';
import * as boardMenus from './menus';
import { columnsOf, followsMove, newBoardId, newColumnId, type BoardColumn, type BoardDoc, type BoardOp, type BoardPlace } from './model';
import { memberTest, repoOf, type Subject } from './rules';
import type { BoardsHandle } from './useBoards';

/** What the window has for it. */
export interface BoardEditsInput {
  /** The project's boards and the one the Board view shows (useBoards.ts). */
  boards: BoardsHandle;
  /** A change to a board, here at once; what the engine turns down is said in a toast. */
  applyBoard: (op: BoardOp) => Promise<unknown>;
  /** Whether boards are on (`[experimental] boards`): off, nothing is offered a board. */
  boardsOn: boolean;
  view: string;
  setView: (view: BoardLayout) => void;
  /** The Board view's layout the window goes back to: the last one shown. */
  lastLayout: BoardLayout;
  /** The project the window shows, and the board's projects. */
  current: string;
  projects: { name: string }[];
  pickProject: (name: string) => void;
  setSelectedKey: (key: string | null) => void;
  setToast: (message: string) => void;
  board: Board | null;
  /** Every worktree the window has, and the project's, for what a board's rule can pick. */
  rawLanes: Lane[];
  projectLanes: Lane[];
  todoStore: { todos: Todo[]; labels: Label[] };
  /** Whether the board shown holds a card, and everything a board's rule could take in. */
  member: (card: string) => boolean;
  allSubjects: Map<string, Subject>;
  now: number;
}

export function useBoardEdits(input: BoardEditsInput) {
  const { boards, applyBoard, boardsOn, view, setView, lastLayout, current, projects, pickProject, setSelectedKey, setToast, board, rawLanes, projectLanes, todoStore, member, allSubjects, now } = input;
  const shownBoard = boards.board;
  /** Add to board › for one thing or several of a kind picked together: each of the project's
   *  boards that would show them (under All projects, every project's and each one's own
   *  project's) checked when it holds them (a pick puts them on or takes them off, `boardChoices`),
   *  saying how many of them it takes where that's fewer than all; and in Kanban, Remove from
   *  board for the board shown. The canvas has its own Remove from board, for what's picked on it. */
  const boardItems = (ref: RefKind, keys: string[]): MenuSpec => {
    if (!boardsOn || keys.length === 0) return [];
    const has = { lanes: rawLanes, todos: todoStore.todos, prs: board?.prs ?? [] };
    const cards = keys.map((key) => ({ card: refId(ref, key), project: projectOfCard(ref, key, has) }));
    return boardMenus.addToBoard(ref, cards, {
      boards: boards.boards,
      current,
      holds: (b) => (b.id === shownBoard?.id ? member : memberTest(b, allSubjects, now)),
      kanban: view === 'kanban' && shownBoard ? { board: shownBoard, member } : null,
      put: (b, yes, list) => void applyBoard({ op: yes ? 'add' : 'remove', id: b.id, cards: list }),
      create: (list) => setBoardEdit({ board: null, with: list }),
    });
  };
  // Making or changing a board, or a column of its Kanban (BoardDialog.tsx).
  const [boardEdit, setBoardEdit] = useState<{ board: BoardDoc | null; with?: string[] } | null>(null);
  const [columnEdit, setColumnEdit] = useState<{ column: BoardColumn | null } | null>(null);
  const showBoard = (id: string) => {
    boards.pick(id);
    setSelectedKey(null);
    if (view !== 'kanban' && view !== 'canvas') setView(lastLayout);
  };
  const saveBoard = (draft: BoardDraft) => {
    const b = boardEdit?.board;
    if (b) {
      if (draft.name !== b.name) void applyBoard({ op: 'rename', id: b.id, name: draft.name });
      if (JSON.stringify(draft.rule) !== JSON.stringify(b.rule)) void applyBoard({ op: 'rule', id: b.id, rule: draft.rule });
      return;
    }
    const id = newBoardId();
    void applyBoard({ op: 'create', board: { id, name: draft.name, project: current, rule: draft.rule, added: boardEdit?.with ?? [], removed: [], columns: draft.columns } });
    boards.pick(id);
    if (view !== 'kanban' && view !== 'canvas') setView(lastLayout);
  };
  /** Moves a board to another project. The window follows the board it shows there, unless it
   *  shows All projects, which lists it still (`followsMove`). */
  const boardPlace = useLatest<BoardPlace>({ project: current, showing: view === 'kanban' || view === 'canvas', board: shownBoard?.id ?? '' });
  const moveBoard = (b: BoardDoc, project: string) => {
    const asked = boardPlace.current;
    void boards.apply({ op: 'project', id: b.id, project }).then(
      () => {
        if (!followsMove(b.id, asked, boardPlace.current)) return;
        boards.pick(b.id, project);
        pickProject(project);
      },
      (e: unknown) => setToast(errorText(e)),
    );
  };
  /** A board's menu, or with none the Board view's: the project's boards to show, a new one, and
   *  the board's settings, moving it to another project as a todo moves (every project but
   *  Elsewhere, and All projects), deleting it. */
  const boardMenu = (b: BoardDoc | null = shownBoard) =>
    void showContextMenu(
      boardMenus.boardMenu(b, {
        boards: boards.boards,
        showing: view === 'kanban' || view === 'canvas' ? (shownBoard?.id ?? null) : null,
        current,
        projects: [...todoProjects(projects), ALL_PROJECTS],
        show: showBoard,
        create: () => setBoardEdit({ board: null }),
        settings: (x) => setBoardEdit({ board: x }),
        move: moveBoard,
        stages: (x) => void applyBoard({ op: 'columns', id: x.id, columns: null }),
        remove: (x) => {
          void applyBoard({ op: 'delete', id: x.id });
          setToast(`Deleted the board ${x.name}. What was on it stays as it was.`);
        },
      }),
    ).catch((e: unknown) => setToast(String(e)));
  // A column's menu (KanbanView.tsx): its settings, folding it, moving it, taking it away.
  const columnMenu = (c: { id: string; column: BoardColumn | null }) => {
    if (!shownBoard) return;
    const spec = boardMenus.columnMenu(columnsOf(shownBoard), c, {
      settings: (column) => setColumnEdit({ column }),
      add: () => setColumnEdit({ column: null }),
      set: (next) => void applyBoard({ op: 'columns', id: shownBoard.id, columns: next }),
    });
    if (spec) void showContextMenu(spec).catch((e: unknown) => setToast(String(e)));
  };
  const saveColumn = (fields: Omit<BoardColumn, 'id'>) => {
    if (!shownBoard) return;
    const cols = columnsOf(shownBoard);
    const editing = columnEdit?.column;
    const next = editing ? cols.map((x) => (x.id === editing.id ? { ...fields, id: x.id } : x)) : [...cols, { ...fields, id: newColumnId(cols) }];
    void applyBoard({ op: 'columns', id: shownBoard.id, columns: next });
  };
  // What a board's rule can pick: the project's labels, repos and accounts.
  const ruleChoices = useMemo(
    () => ({
      labels: todoStore.labels,
      repos: [...new Map(projectLanes.map((l) => [repoOf(l), { id: repoOf(l), label: l.repo }])).values()],
      accounts: [...new Set(projectLanes.flatMap((l) => l.sessions.map((s) => s.account)))].sort(),
    }),
    [todoStore.labels, projectLanes],
  );
  return { boardItems, boardEdit, setBoardEdit, columnEdit, setColumnEdit, showBoard, saveBoard, boardMenu, columnMenu, saveColumn, ruleChoices };
}
