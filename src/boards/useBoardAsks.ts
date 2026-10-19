// The main window's answers to `codealong board show|move|arrange` (cli.rs), which the engine
// hands it (boards.rs `ask`): the window works a board out (its columns, boardLists.ts
// `kanbanOf`; its canvas's layout, canvas/model.ts), so what a board holds, moving a card on its
// Kanban, and arranging its canvas in the frames an agent names are worked out here. The canvas
// this window shows changes through its own handle; another board's is read, changed and saved,
// and a window showing that one reads it again (canvas/useCanvas.ts).
//
// A request is answered from the board as the engine has it when it's asked, not as the window
// last drew it: a window that's hidden, covered or behind a locked screen isn't drawn (WebKit holds
// its page back), so what its last render had can be hours old, or from before the first scan.

import { useEffect } from 'react';
import { boardAnswer, boardsList, getBoard, onBoardAsk, type BoardAsk } from '../api';
import { kanbanOf, NO_COLUMN, unarchived, type KanbanCard, type KanbanColumn } from '../boardLists';
import { arrange, emptyDoc, entitiesOf, outline, refId, type ArrangeFrame, type ArrangeRest, type OnBoard } from '../canvas/model';
import type { CanvasHandle } from '../canvas/useCanvas';
import { readCanvas, saveCanvas, seed } from '../canvas/useCanvas';
import { withDevices, type DeviceFilter } from '../devices';
import { boardOrderWith, columnWith, inSlots, kanbanCardId, kanbanDrop, type KanbanMove } from '../kanban';
import { useLatest } from '../latest';
import { ALL_PROJECTS, inProject } from '../quick';
import { errorText } from '../report';
import type { TodoOp } from '../todos';
import type { Board, Lane } from '../types';
import type { SessionItem } from '../ui/SessionsView';
import { isProjects, projectBoard, readBoard, type BoardDoc, type BoardOp } from './model';

/** What `codealong board …` asks for, as boards.rs `Ask` sends it. */
type Ask = { kind: 'show'; layout: 'kanban' | 'canvas' } | { kind: 'arrange'; frames: ArrangeFrame[]; rest: ArrangeRest } | { kind: 'move'; card: string; to: string; before?: string; dryRun?: boolean };

export interface BoardAsks {
  /** Other devices' worktrees, and the devices the window shows, which the board adds them to. */
  deviceLanes: Lane[];
  deviceFilter: DeviceFilter;
  /** The board this window shows, and its canvas. */
  shown: BoardDoc;
  canvas: CanvasHandle;
  /** A change to a board, as the window makes one (useBoards.ts `apply`). */
  apply: (op: BoardOp) => Promise<unknown>;
  /** A change to todos, through the window's store. */
  run: (op: TodoOp) => void;
  /** Mark as done on a session, as its menu does it. */
  markDone: (item: SessionItem) => void;
}

/** Answers each `codealong board …` request the engine hands this window. */
export function useBoardAsks(asks: BoardAsks): void {
  const latest = useLatest(asks);
  useEffect(
    () =>
      onBoardAsk((ask) => {
        answer(ask, latest)
          .then(
            (result) => boardAnswer(ask.id, { result }),
            (e: unknown) => boardAnswer(ask.id, { error: errorText(e) }),
          )
          // The engine stops waiting by itself, and tells the command the window didn't answer.
          .catch(() => {});
      }),
    [latest],
  );
}

/** The board asked about, as the engine has it now. */
async function boardAsked(ask: BoardAsk): Promise<BoardDoc> {
  const project = ask.project;
  const found = (await boardsList(project)).map(readBoard).find((b) => b.id === ask.board);
  if (found) return found;
  const own = projectBoard(project);
  if (own.id === ask.board) return own;
  throw new Error(`There's no board ${ask.board} in ${project}.`);
}

/** What the board holds, as the canvas takes it. */
function holds(doc: BoardDoc, member: (card: string) => boolean): OnBoard {
  return { has: member, added: new Set(doc.added), removed: new Set(doc.removed) };
}

const named = (doc: BoardDoc) => ({ id: doc.id, name: doc.name, project: doc.project });

async function answer(ask: BoardAsk, latest: { readonly current: BoardAsks }): Promise<unknown> {
  const board = await getBoard();
  if (board.loading) throw new Error('Codealong is still reading the board. Ask again in a moment.');
  let doc = await boardAsked(ask);
  const { deviceLanes, deviceFilter } = latest.current;
  const lanes = unarchived(withDevices(board.lanes, deviceLanes, deviceFilter));
  const now = Date.now();
  const op = ask.ask as Ask;
  if (op.kind === 'move') return move(op, doc, board, lanes, now, latest.current);
  if (op.kind === 'show' && op.layout === 'kanban') {
    const { columns } = kanbanOf(board, lanes, doc, now);
    return { board: named(doc), rule: doc.rule, columns: columns.map(shownColumn) };
  }
  // A card an arrange names that isn't on the board is put on it.
  if (op.kind === 'arrange') {
    const { member } = kanbanOf(board, lanes, doc, now);
    const cards = op.frames.flatMap((f) => f.cards.map((c) => refId(c.ref, c.key))).filter((c) => !member(c));
    if (cards.length > 0) {
      await latest.current.apply({ op: 'add', id: doc.id, cards: [...new Set(cards)] });
      doc = await boardAsked(ask);
    }
  }
  const { member } = kanbanOf(board, lanes, doc, now);
  const projectLanes = lanes.filter((l) => inProject(l, doc.project));
  const e = entitiesOf(board, projectLanes, board.todos, (p) => doc.project === ALL_PROJECTS || p === doc.project);
  const pinned = new Set(board.pinnedWorktrees);
  const on = holds(doc, member);
  const file = await readCanvas(doc.id);
  // The window's own document once it holds the board's, which may have a change still to save,
  // else the file, read again if the window saved the board meanwhile (`readCanvas`). Taken after
  // every wait, with nothing waited on from here to the save, so a change made in the window
  // meanwhile is in what's arranged, and the board is still the one the window shows if it's put
  // there.
  const o = latest.current;
  const here = doc.id === o.shown.id;
  const canvas = here && o.canvas.loaded ? o.canvas.doc : file;
  if (op.kind === 'show') return { board: named(doc), exists: !!canvas, ...outline(canvas ?? emptyDoc(), e) };
  if (op.kind === 'arrange') {
    // A board whose canvas was never shown starts as it would have: tidied.
    const done = arrange(canvas ?? seed(e, now, pinned, on), e, op.frames, op.rest, now, pinned, on);
    // Saved before the command hears it's done, so a failure to save is its answer. The board the
    // window shows goes through its handle, so the window holds what was saved.
    await (here ? o.canvas.put(done.doc) : saveCanvas(doc.id, done.doc));
    return { board: named(doc), frames: done.frames, missing: done.missing, rest: done.rest };
  }
  throw new Error('This Codealong doesn’t know that board request: the command is newer than the app.');
}

/** A column as `codealong board show` prints it. */
function shownColumn(c: KanbanColumn) {
  return {
    id: c.id,
    name: c.label,
    ...(c.column?.rule ? { rule: c.column.rule } : {}),
    sort: c.column?.sort ?? 'move',
    folded: !!c.column?.folded,
    cards: c.cards.map((card) => ({
      ref: card.kind === 'todo' ? 'todo' : 'session',
      key: card.kind === 'todo' ? card.todo.id : card.session.key,
      title: (card.kind === 'todo' ? card.todo.title : card.session.title) || 'Untitled',
      ...(card.kind === 'todo' ? { sessions: card.todo.sessions } : {}),
    })),
  };
}

/** Moves a card on a board's Kanban to a column, as dragging it there does (kanban.ts), or says
 *  what that would do. */
async function move(op: Extract<Ask, { kind: 'move' }>, doc: BoardDoc, board: Board, lanes: Lane[], now: number, acts: BoardAsks): Promise<unknown> {
  const { columns, subjects } = kanbanOf(board, lanes, doc, now);
  const from = columns.find((c) => c.cards.some((x) => kanbanCardId(x) === op.card));
  const card = from?.cards.find((x) => kanbanCardId(x) === op.card);
  if (!from || !card) throw new Error(`${op.card} isn't on ${doc.name}'s Kanban. \`codealong board show\` lists what is.`);
  const want = op.to.trim().toLowerCase();
  const to = columns.find((c) => c.id !== NO_COLUMN && (c.id.toLowerCase() === want || c.label.toLowerCase() === want));
  if (!to) throw new Error(`${doc.name} has no column ${op.to}. Its columns are ${columns.filter((c) => c.id !== NO_COLUMN).map((c) => c.label).join(', ')}.`);
  const before = op.before ? to.cards.findIndex((x) => kanbanCardId(x) === op.before) : -1;
  if (op.before && before < 0) throw new Error(`${op.before} isn't in ${to.label}.`);
  // A place in the column is yours to give only where the column keeps your order.
  const ordered = to.column?.sort === 'hand';
  const index = op.before && ordered ? before : null;
  const drop = kanbanDrop(card, from, to, null, index ?? (from.id === to.id && ordered ? to.cards.length : null), { columns: columns.flatMap((c) => c.column ?? []), placed: doc.placed, subjects, todos: board.todos, now });
  if (!drop || drop.moves.length === 0) throw new Error(from.id === to.id ? `It's in ${to.label} already.` : `Moving it to ${to.label} does nothing: no change would put it there.`);
  if (drop.moves.some((m) => m.kind === 'start')) throw new Error('Moving a todo from To do to In progress starts a session, which the command doesn’t do: run `codealong todo start` in the session that takes it on.');
  const title = (card.kind === 'todo' ? card.todo.title : card.session.title) || 'Untitled';
  const said = `${drop.word ?? `Move to ${to.label}`}: “${title}”.`;
  if (op.dryRun) return { said: `Would ${said.charAt(0).toLowerCase()}${said.slice(1)}`, column: to.label, done: false };
  await doMoves(card, drop.moves, to, doc, board, acts);
  return { said: `${said}`, column: to.label, done: true };
}

/** Does a drop's moves for the command, as applyKanbanDrop does them for a drag, without a toast. */
async function doMoves(card: KanbanCard, moves: KanbanMove[], to: KanbanColumn, doc: BoardDoc, board: Board, acts: BoardAsks): Promise<void> {
  const id = kanbanCardId(card);
  for (const m of moves) {
    if (card.kind === 'agent') {
      if (m.kind === 'done') acts.markDone(card);
    } else if (m.kind === 'status') acts.run({ op: 'update', id: card.todo.id, patch: { status: m.status } });
    else if (m.kind === 'review') acts.run({ op: 'update', id: card.todo.id, patch: { review: m.on ? { note: '', since: 0 } : null, ...(m.on && card.todo.status === 'done' ? { status: 'in-progress' as const } : {}) } });
    else if (m.kind === 'labels') acts.run({ op: 'update', id: card.todo.id, patch: { labels: [...card.todo.labels.filter((l) => !m.remove.includes(l)), ...m.add.filter((l) => !card.todo.labels.includes(l))] } });
    if (m.kind === 'place') await acts.apply({ op: 'place', id: doc.id, card: id, column: m.column });
    if (m.kind === 'order') {
      const seq = columnWith(to.cards.map(kanbanCardId), id, m.index);
      // A project's own board keeps the Todo view's order.
      if (isProjects(doc)) {
        if (card.kind === 'todo') acts.run({ op: 'order', ids: inSlots(board.todos.map((t) => t.id), seq.flatMap((c) => (c.startsWith('todo:') ? [c.slice(5)] : []))) });
      } else await acts.apply({ op: 'order', id: doc.id, order: boardOrderWith(doc.order ?? [], seq) });
    }
  }
}
